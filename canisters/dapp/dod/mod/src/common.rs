pub const CMC_CAN_ID: &str = "rkp4c-7iaaa-aaaaa-aaaca-cai";
pub const ICP_CAN_ID: &str = "ryjl3-tyaaa-aaaaa-aaaba-cai";
pub const CYCLES_CAN_ID: &str = "um5iw-rqaaa-aaaaq-qaaba-cai";
pub const MEMO_TOP_UP_CANISTER: u64 = 1347768404_u64;
pub const ICP_FEE: u64 = 10_000u64;
pub const CYCLES_BURNER_FEE: u128 = 1_000_000_000_u128;
pub const BURN_ORDERS_LIMIT: u128 = 500;
pub const CYCLES_CREATE_FEE: u128 = 2_000_000_000_000u128;
pub const MIN_ICP_STAKE_E8S_U64: u64 = 100_0000;

pub const ONE_MINUTE_NS: u64 = 1_000_000_000 * 60;
pub const ONE_HOUR_NS: u64 = ONE_MINUTE_NS * 60;
pub const ONE_DAY_NS: u64 = ONE_HOUR_NS * 24;
pub const ONE_WEEK_NS: u64 = ONE_DAY_NS * 7;
pub const ONE_MONTH_NS: u64 = ONE_WEEK_NS * 30;

use candid::{CandidType, Deserialize, Nat, Principal};
use ic_cdk::api::call::CallResult;
use ic_cdk::call;
use std::fmt::Display;

pub type TimestampNs = u64;

pub const TCYCLE_POS_ROUND_BASE_FEE: u64 = 50_000_000_000_u64;

pub struct CMCClient(pub Principal);

#[derive(CandidType, Deserialize)]
pub struct NotifyTopUpRequest {
    pub block_index: u64,
    pub canister_id: Principal,
}

#[derive(CandidType, Deserialize, Debug)]
pub enum NotifyTopUpError {
    Refunded {
        block_index: Option<u64>,
        reason: String,
    },
    InvalidTransaction(String),
    Other {
        error_message: String,
        error_code: u64,
    },
    Processing,
    TransactionTooOld(u64),
}

impl CMCClient {
    pub async fn notify_top_up(
        &self,
        req: NotifyTopUpRequest,
    ) -> CallResult<(Result<Nat, NotifyTopUpError>,)> {
        call(self.0, "notify_top_up", (req,)).await
    }
}

#[derive(CandidType, Deserialize, Debug)]
pub enum UserError {
    InsufficientBalance,
}

impl Display for UserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UserError::InsufficientBalance => write!(f, "Insufficient balance"),
        }
    }
}

pub const MEMO_TOP_UP: u64 = 4040404040401_u64;
pub const MEMO_TRANSFER: u64 = 4040404040402_u64;
pub const MEMO_BURN_DOD: u64 = 4040404040403_u64;
pub const MEMO_BURN_CYCLES: u64 = 4040404040404_u64;
