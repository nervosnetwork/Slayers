use crate::address::{Address, MAINNET_PREFIX};
use crate::input::{serialize_multisig_lock_args, LockRecord, RawRecord};
use crate::{DEFAULT_CODE_HASH, MULTISIG_CODE_HASH};
use bech32::{self, ToBase32};
use ckb_types::{bytes::Bytes, H256};
use failure::Error;
use serde_derive::Serialize;
use std::convert::{TryFrom, TryInto};
use std::fs::File;
use std::str::FromStr;

#[derive(Debug, Serialize)]
pub struct Output {
    pub address: String,
    pub capacity: u64,
    pub lock: Option<String>,
    pub code_hash: String,
    pub args: String,
    pub mainnet_address: String,
}

impl TryFrom<RawRecord> for Output {
    type Error = Error;

    fn try_from(record: RawRecord) -> Result<Self, Self::Error> {
        let RawRecord { address, capacity } = record;
        let decode_address = Address::from_str(&address)?;
        Ok(Output {
            address,
            capacity,
            lock: None,
            code_hash: DEFAULT_CODE_HASH.to_string(),
            args: format!(
                "0x{}",
                faster_hex::hex_string(&decode_address.args[..]).unwrap()
            ),
            mainnet_address: decode_address.mainnet_short_format()?,
        })
    }
}

fn full_payload_format(args: Bytes) -> Result<String, Error> {
    let mut payload = vec![4u8]; // Type
    payload.extend_from_slice(H256::from_str(&MULTISIG_CODE_HASH[2..]).unwrap().as_bytes());
    payload.extend_from_slice(&args);
    bech32::encode(MAINNET_PREFIX, payload.to_base32()).map_err(Into::into)
}

fn convert_lock_output(record: LockRecord, target: u64) -> Result<Output, Error> {
    let LockRecord {
        address,
        capacity,
        lock,
    } = record;

    if let Some(ref date) = &lock {
        let args = serialize_multisig_lock_args(&address, date, target)?;
        let mainnet_address = full_payload_format(args.clone())?;
        Ok(Output {
            address,
            capacity,
            lock,
            code_hash: MULTISIG_CODE_HASH.to_string(),
            args: format!("0x{}", faster_hex::hex_string(&args[..]).unwrap()),
            mainnet_address,
        })
    } else {
        let decode_address = Address::from_str(&address)?;

        Ok(Output {
            address,
            capacity,
            lock,
            code_hash: DEFAULT_CODE_HASH.to_string(),
            args: format!(
                "0x{}",
                faster_hex::hex_string(&decode_address.args[..]).unwrap()
            ),
            mainnet_address: decode_address.mainnet_short_format()?,
        })
    }
}

pub fn write_incentives_output<R: IntoIterator<Item = RawRecord>>(
    wtr: &mut csv::Writer<File>,
    records: R,
) -> Result<(), Error> {
    for record in records
        .into_iter()
        .filter_map(|record| TryInto::<Output>::try_into(record).ok())
    {
        wtr.serialize(record)?;
    }
    Ok(())
}

pub fn write_allocate_output<R: IntoIterator<Item = LockRecord>>(
    wtr: &mut csv::Writer<File>,
    records: R,
    target: u64,
) -> Result<(), Error> {
    for record in records
        .into_iter()
        .filter_map(|record| convert_lock_output(record, target).ok())
    {
        wtr.serialize(record)?;
    }
    Ok(())
}
