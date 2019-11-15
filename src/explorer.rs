use crate::address::Address;
use crate::output::Output;
use crate::rpc::RpcClient;
use crate::DEFAULT_CODE_HASH;
use chrono::{prelude::*, Duration};
use ckb_rational::RationalU256;
use ckb_types::{
    bytes::Bytes,
    core::{capacity_bytes, BlockView, Capacity, HeaderView},
    packed::{Byte32, CellbaseWitness},
    prelude::*,
    utilities::{compact_to_difficulty, difficulty_to_compact},
    U256,
};
use failure::Error;
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fs::File;
use std::ops::Add;
use std::process::exit;

const TOTAL_REWARD: Capacity = capacity_bytes!(18_000_000);
const THRESHOLD: Capacity = capacity_bytes!(1_000);
const METRIC_EPOCH: u64 = 4;
const BYTE_SHANNONS: u64 = 100_000_000;

pub struct Explorer {
    rpc: RpcClient,
    target: u64,
}

impl Explorer {
    pub fn new(url: &str, target: u64) -> Explorer {
        Explorer {
            rpc: RpcClient::new(url),
            target,
        }
    }

    pub fn collect(
        &self,
        map: &mut BTreeMap<Bytes, Capacity>,
        output: &mut Option<csv::Writer<File>>,
    ) -> Result<(u64, u32, Byte32, u64), Error> {
        let tip_header: HeaderView = self.rpc.get_tip_header()?.into();
        let tip_epoch = tip_header.epoch();
        if (tip_epoch.number() < (self.target + 1)) || tip_epoch.index() < 11 {
            self.estimate_launch_time(tip_header)?;
            exit(1);
        }

        let next_epoch = self
            .rpc
            .get_epoch_by_number((self.target + 1).into())?
            .unwrap_or_else(|| exit(1));

        let next_epoch_start: u64 = next_epoch.start_number.into();

        let endpoint = next_epoch_start - 1;

        let mut rewards = HashMap::with_capacity(42);
        let mut windows = VecDeque::with_capacity(10);

        let progress_bar = ProgressBar::new(endpoint + 11);
        progress_bar.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:60.cyan/blue} {pos:>7}/{len:7} {msg}")
                .progress_chars("##-"),
        );

        for num in 1..=11 {
            progress_bar.inc(1);
            if let Some(block) = self.rpc.get_block_by_number(num.into())? {
                let block: BlockView = block.into();
                windows.push_back(block);
            } else {
                exit(1);
            }
        }

        for cursor in 12..=(endpoint + 11) {
            progress_bar.inc(1);
            if let Some(block) = self.rpc.get_block_by_number(cursor.into())? {
                let block: BlockView = block.into();
                windows.push_back(block);
            } else {
                exit(1);
            }

            let hash = self
                .rpc
                .get_block_hash(cursor.into())?
                .unwrap_or_else(|| exit(1));

            let reward = self
                .rpc
                .get_cellbase_output_capacity_details(hash)?
                .unwrap_or_else(|| exit(1));
            let target_lock = CellbaseWitness::from_slice(
                &windows[0].transactions()[0]
                    .witnesses()
                    .get(0)
                    .expect("target witness exist")
                    .raw_data(),
            )
            .expect("cellbase loaded from store should has non-empty witness")
            .lock();

            let entry = rewards.entry(target_lock).or_insert_with(Capacity::zero);
            let primary: u64 = reward.primary.into();

            *entry = entry.safe_add(primary)?;
            if cursor != endpoint + 11 {
                windows.pop_front();
            }
        }
        let chosen_one = windows.pop_front().unwrap_or_else(|| exit(1));
        rewards.retain(|_, &mut r| r > THRESHOLD);

        let total = rewards
            .iter()
            .map(|(_, capacity)| *capacity)
            .try_fold(Capacity::zero(), Capacity::safe_add)?;

        if let Some(wtr) = output.as_mut() {
            wtr.write_record(&["#round5.stage3", "", "", "", "", ""])
                .unwrap();
        }

        for (lock, capacity) in rewards {
            let ratio =
                RationalU256::new(U256::from(capacity.as_u64()), U256::from(total.as_u64()));
            let total = RationalU256::new(U256::from(TOTAL_REWARD.as_u64()), U256::one());
            let reward_ckb = get_low64(&(total * ratio).into_u256()) / BYTE_SHANNONS;
            let reward = reward_ckb * BYTE_SHANNONS;

            let entry = map
                .entry(lock.args().raw_data())
                .or_insert_with(Capacity::zero);
            *entry = entry.safe_add(reward)?;

            if let Some(wtr) = output.as_mut() {
                let address = Address::new(lock.args().raw_data());
                let output = Output {
                    address: address.testnet_short_format()?,
                    capacity: reward_ckb,
                    lock: None,
                    code_hash: DEFAULT_CODE_HASH.to_string(),
                    args: format!("0x{}", faster_hex::hex_string(&address.args[..]).unwrap()),
                    mainnet_address: address.mainnet_short_format()?,
                };
                wtr.serialize(output)?;
            }
        }

        let epochs: Vec<_> = (0..METRIC_EPOCH)
            .map(|i| {
                self.rpc
                    .get_epoch_by_number((self.target - i).into())
                    .unwrap_or_else(|_| exit(1))
                    .unwrap_or_else(|| exit(1))
            })
            .collect();

        let avg_diff: U256 = epochs
            .iter()
            .map(|epoch| compact_to_difficulty(epoch.compact_target.into()))
            .fold(U256::zero(), U256::add)
            / U256::from(METRIC_EPOCH);

        let diff = (avg_diff * U256::from(3u64) / U256::from(2u64)) * U256::from(total.as_u64())
            / U256::from(TOTAL_REWARD.as_u64());

        let compact_target = difficulty_to_compact(diff);

        progress_bar.finish();
        Ok((
            chosen_one.timestamp(),
            compact_target,
            chosen_one.hash(),
            epochs[0].length.into(),
        ))
    }

    pub fn estimate_launch_time(&self, tip_header: HeaderView) -> Result<(), Error> {
        let now = Local::now();
        let tip_epoch = tip_header.epoch();

        let avg_epoch_duration = if tip_epoch.number() < METRIC_EPOCH {
            4 * 3600
        } else {
            // get average elapsed time in the last four full epochs
            let first_epoch = self
                .rpc
                .get_epoch_by_number((tip_epoch.number() - METRIC_EPOCH).into())
                .unwrap_or_else(|_| exit(1))
                .unwrap_or_else(|| exit(1));
            let first_block = self
                .rpc
                .get_header_by_number(first_epoch.start_number)?
                .unwrap_or_else(|| exit(1));
            let last_block = self
                .rpc
                .get_header_by_number(
                    (Into::<u64>::into(tip_header.number()) - tip_epoch.index()).into(),
                )?
                .unwrap_or_else(|| exit(1));
            let t1: u64 = first_block.inner.timestamp.into();
            let t2: u64 = last_block.inner.timestamp.into();
            (t2 - t1) / METRIC_EPOCH / 1000
        };

        let remaining_seconds = (self.target.saturating_sub(tip_epoch.number()))
            * avg_epoch_duration
            + avg_epoch_duration * (tip_epoch.length() - tip_epoch.index() + 11)
                / tip_epoch.length();
        let remaining_duration = Duration::seconds(remaining_seconds as i64);

        println!(
            "Lina is not ready yet. Please wait for the 11st block in epoch {}.",
            self.target + 1
        );
        print!("Estimated remaining time: ");
        if remaining_seconds > 86400 {
            print!("{} days ", remaining_seconds / 86400)
        }
        println!(
            "{:02}h{:02}m{:02}s",
            remaining_seconds / 3600 % 24,
            remaining_seconds / 60 % 60,
            remaining_seconds % 60
        );
        println!("Estimated launch time: {}", now + remaining_duration);

        Ok(())
    }
}

fn get_low64(u256: &U256) -> u64 {
    u256.0[0]
}
