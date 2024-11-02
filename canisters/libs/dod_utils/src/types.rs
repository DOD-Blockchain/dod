use crate::bitwork::Bitwork;
use candid::{CandidType, Decode, Deserialize, Encode, Principal};
use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use serde::Serialize;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::BTreeMap;

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub struct BtcAddress(pub String);

impl Storable for BtcAddress {
    fn to_bytes(&self) -> Cow<[u8]> {
        self.0.to_bytes()
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Self(String::from_bytes(bytes))
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: 64 * 2,
        is_fixed_size: false,
    };
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum MinerStatus {
    Activate,
    Deactivate,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct MinerInfo {
    pub owner: Principal,
    pub status: MinerStatus,
    pub ecdsa_pubkey: Vec<u8>,
    pub btc_address: String,
    pub reward_cycles: Option<u128>, // cycles
    pub claimed_dod: u64,            // dod coin
    pub total_dod: u64,              // dod coin
}

impl Storable for MinerInfo {
    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Owned(Encode!(self).unwrap())
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).unwrap()
    }
    const BOUND: Bound = Bound::Bounded {
        max_size: 256,
        is_fixed_size: false,
    };
}

pub type Height = u64;
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct BlockData {
    pub height: Height,
    pub rewards: u64,
    pub winner: Option<MinerInfo>,
    pub difficulty: Bitwork,
    pub hash: Vec<u8>,
    pub block_time: u64,
    pub next_block_time: u64,
    pub history: bool,
    pub cycle_burned: u128,
    pub dod_burned: u64,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum DifficultyStatus {
    Increase,
    Decrease,
    Keep,
}

impl Storable for BlockData {
    // serialize the struct to bytes
    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Owned(Encode!(self).unwrap())
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).unwrap()
    }
    const BOUND: Bound = Bound::Bounded {
        max_size: 512,
        is_fixed_size: false,
    };
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct BlockSigs {
    pub commit_tx: Vec<u8>,
    pub reveal_tx: Vec<u8>,
}

impl Storable for BlockSigs {
    // serialize the struct to bytes
    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Owned(Encode!(self).unwrap())
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).unwrap()
    }
    const BOUND: Bound = Bound::Bounded {
        max_size: 2048,
        is_fixed_size: false,
    };
}

#[derive(CandidType, Serialize, Deserialize, Clone)]
pub struct BootStrapParams {
    pub dod_token_canister: Option<Principal>,
    pub dod_block_sub_account: Vec<u8>,
    pub block_timer: u64,
    pub difficulty_epoch: u64,
    pub default_rewards: u64,
    pub start_difficulty: Option<Bitwork>,
    pub halving_settings: Option<HalvingSettings>,
    pub difficulty_step: Option<u64>,
}

#[derive(CandidType, Serialize, Deserialize, Clone)]
pub struct MinerSubmitPayload {
    pub btc_address: String,
    pub signed_commit_psbt: String,
    pub signed_reveal_psbt: String,
    pub cycles_price: u128,
}

#[derive(CandidType, Serialize, Deserialize, Clone)]
pub struct MinerSubmitResponse {
    pub block_height: u64,
    pub cycles_price: u128,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct MinterCandidates {
    pub candidates: BTreeMap<String, MinerCandidate>,
}

impl Storable for MinterCandidates {
    // serialize the struct to bytes
    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Owned(Encode!(self).unwrap())
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).unwrap()
    }
    const BOUND: Bound = Bound::Unbounded;
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct MinerCandidate {
    pub btc_address: String,
    pub submit_time: u64,
    pub cycles_price: u128,
    pub signed_commit_psbt: String,
    pub signed_reveal_psbt: String,
}

impl Ord for MinerCandidate {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.cycles_price.cmp(&other.cycles_price) {
            Ordering::Equal => self.submit_time.cmp(&other.submit_time),
            other => other,
        }
    }
}

impl PartialOrd for MinerCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub struct MinerCandidateKey {
    pub btc_address: String,
    pub block: u64,
}

impl Storable for MinerCandidateKey {
    // serialize the struct to bytes
    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Owned(Encode!(self).unwrap())
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).unwrap()
    }
    const BOUND: Bound = Bound::Bounded {
        max_size: 128,
        is_fixed_size: false,
    };
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub struct BlockOrders {
    pub block_height: u64,
    pub orders: BTreeMap<UserOrdersKey, u128>,
}

impl Storable for BlockOrders {
    // serialize the struct to bytes
    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Owned(Encode!(self).unwrap())
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).unwrap()
    }
    const BOUND: Bound = Bound::Unbounded;
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub enum UserType {
    Miner,
    User,
    Treasury,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub struct UserOrdersKey {
    pub p: Principal,
    pub u: UserType,
}

impl Storable for crate::types::UserOrdersKey {
    // serialize the struct to bytes
    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Owned(Encode!(self).unwrap())
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).unwrap()
    }
    const BOUND: Bound = Bound::Bounded {
        max_size: 128,
        is_fixed_size: false,
    };
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub struct UserOrders {
    pub principal: Principal,
    pub orders: BTreeMap<u64, u128>,
    pub user_type: UserType,
}

impl Storable for crate::types::UserOrders {
    // serialize the struct to bytes
    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Owned(Encode!(self).unwrap())
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).unwrap()
    }
    const BOUND: Bound = Bound::Unbounded;
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct UserBlockOrder {
    pub block: u64,
    pub amount: u128,
    pub share: f64,
    pub reward: u64,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct UserBlockOrderRes {
    pub total: u64,
    pub from: u64,
    pub to: u64,
    pub data: Vec<UserBlockOrder>,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct HalvingSettings {
    pub interval: u64,
    pub ratio: f64,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct DodCanisters {
    pub ledger: Principal,
    pub index: Principal,
    pub archive: Principal,
}
pub type BlockNumber = u64;
pub type BlockRange = (BlockNumber, BlockNumber);

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub struct NewBlockOrderValue {
    pub r: BlockRange,
    pub v: u128,
}

impl Storable for crate::types::NewBlockOrderValue {
    // serialize the struct to bytes
    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Owned(Encode!(self).unwrap())
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).unwrap()
    }
    const BOUND: Bound = Bound::Bounded {
        max_size: 128,
        is_fixed_size: false,
    };
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct UserBlockOrderData {
    pub height: u64,
    pub amount: u128, // cycles_amount
    pub share: f64,   // cycles_share
    pub reward: u64,  // dod reward
    pub user: Principal,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct BlockDataFull {
    pub block: BlockData,
    pub user_data: Vec<UserBlockOrderData>,
    pub miners: Vec<MinerCandidateExt>,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct MinerCandidateExt {
    pub miner_principal: Principal,
    pub btc_address: String,
    pub submit_time: u64,
    pub cycles_price: u128,
    pub signed_commit_psbt: String,
    pub signed_reveal_psbt: String,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum OrderStatus {
    Pending,
    Filled,
    Cancelled,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct OrderDetail {
    pub value: u128,
    pub status: OrderStatus,
}

impl Storable for crate::types::OrderDetail {
    // serialize the struct to bytes
    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Owned(Encode!(self).unwrap())
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).unwrap()
    }
    const BOUND: Bound = Bound::Bounded {
        max_size: 128,
        is_fixed_size: false,
    };
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct MinerBlockData {
    pub block_height: u64,
    pub winner: bool,
    pub cycles_price: u128,
    pub submit_time: u64,
    pub difficulty: Bitwork,
}
