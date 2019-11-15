use bech32::{self, FromBase32, ToBase32};
use ckb_types::bytes::Bytes;
use failure::{Error, Fail};
use std::fmt;

pub const TESTNET_PREFIX: &str = "ckt";
pub const MAINNET_PREFIX: &str = "ckb";

#[derive(Hash, Eq, PartialEq, Debug, Clone)]
pub struct Address {
    pub args: Bytes,
}

#[derive(Hash, Eq, PartialEq, Debug, Clone, Fail)]
pub struct AddressError(String);

impl fmt::Display for AddressError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Address {
    pub fn new(args: Bytes) -> Address {
        Address { args }
    }

    pub fn from_str(input: &str) -> Result<Address, Error> {
        let (hrp, data) = bech32::decode(input).map_err(|_| AddressError(input.to_string()))?;
        let data = Vec::<u8>::from_base32(&data).map_err(|_| AddressError(input.to_string()))?;
        if hrp != TESTNET_PREFIX && hrp != MAINNET_PREFIX {
            return Err(AddressError(format!("Invalid address {} hrp: {}", input, hrp)).into());
        }
        if data.len() == 22 {
            if data[0] != 0x01 {
                // short version for locks with popular code_hash
                return Err(
                    AddressError(format!("Invalid address {} type: {}", input, data[0])).into(),
                );
            }
            if data[1] != 0x00 {
                // SECP256K1 + blake160
                return Err(AddressError(format!(
                    "Invalid address {} code hash index: {}",
                    input, data[1]
                ))
                .into());
            }
            Ok(Address::new(Bytes::from(&data[2..22])))
        } else if data.len() == 25 {
            if &data[0..5] != b"\x01P2PH" {
                return Err(AddressError(format!(
                    "Invalid address {} format type: {:?}",
                    input,
                    &data[0..5]
                ))
                .into());
            }
            Ok(Address::new(Bytes::from(&data[5..25])))
        } else {
            Err(AddressError(format!(
                "Invalid address {} data length: {}",
                input,
                data.len()
            ))
            .into())
        }
    }

    pub fn testnet_short_format(&self) -> Result<String, Error> {
        let mut payload = vec![1u8, 0];
        payload.extend_from_slice(&self.args);
        bech32::encode(TESTNET_PREFIX, payload.to_base32()).map_err(Into::into)
    }

    pub fn mainnet_short_format(&self) -> Result<String, Error> {
        let mut payload = vec![1u8, 0];
        payload.extend_from_slice(&self.args);
        bech32::encode(MAINNET_PREFIX, payload.to_base32()).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_address_convert() {
        let address = Address::from_str("ckt1qyq9xcl8cg8supmzzy0szazepu89832xq2tsjm3el2").unwrap();
        assert_eq!(
            &address.mainnet_short_format().unwrap(),
            "ckb1qyq9xcl8cg8supmzzy0szazepu89832xq2ts070xnk"
        );

        let address = Address::from_str("ckb1qyq9xcl8cg8supmzzy0szazepu89832xq2ts070xnk").unwrap();
        assert_eq!(
            &address.testnet_short_format().unwrap(),
            "ckt1qyq9xcl8cg8supmzzy0szazepu89832xq2tsjm3el2"
        );
    }
}
