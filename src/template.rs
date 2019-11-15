use serde_derive::Serialize;

#[derive(Debug, Serialize)]
pub struct Spec {
    pub timestamp: u64,
    pub compact_target: String,
    pub message: String,
    pub epoch_length: u64,
    pub allocate: Vec<IssuedCell>,
    pub foundation_reserve: Option<IssuedCell>,
    pub testnet_incentives: Vec<IssuedCell>,
}

#[derive(Debug, Serialize)]
pub struct IssuedCell {
    pub capacity: u64,
    pub code_hash: String,
    pub args: String,
}
