use candid::{CandidType, Nat, Principal};
use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

use crate::service::DodService;
use candid::{Decode, Encode};
use ego_types::app_info::AppInfo;
use ego_types::registry::Registry;
use ego_types::user::User;
use ic_ledger_types::Subaccount;
use icrc_ledger_types::icrc::generic_metadata_value::MetadataValue;
use icrc_ledger_types::icrc1::account::Account;

#[allow(dead_code)]
const MAX_STATE_SIZE: u32 = 2 * 1024 * 1024;
const MAX_USER_PROFILE_SIZE: u32 = 1 * 1024 * 1024;
const MAX_USER_WALLET_SIZE: u32 = 1 * 1024 * 1024;

#[derive(CandidType, Serialize, Deserialize)]
pub struct StableState {
    pub users: Option<User>,
    pub registry: Option<Registry>,
    pub app_info: Option<AppInfo>,
    pub dod_service: Option<DodService>,
}

impl Storable for StableState {
    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Owned(Encode!(self).unwrap())
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).unwrap()
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: MAX_USER_WALLET_SIZE,
        is_fixed_size: false,
    };
}

impl Default for StableState {
    fn default() -> Self {
        StableState {
            users: None,
            registry: None,
            app_info: None,
            dod_service: None,
        }
    }
}

#[derive(CandidType, Deserialize)]
pub struct UserProfile {
    user_id: u16,
    user_name: String,
}

impl Storable for UserProfile {
    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Owned(Encode!(self).unwrap())
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).unwrap()
    }
    const BOUND: Bound = Bound::Bounded {
        max_size: MAX_USER_PROFILE_SIZE,
        is_fixed_size: false,
    };
}

#[derive(CandidType, Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq, Clone)]
pub struct UserDetail {
    pub(crate) principal: Principal,
    pub(crate) subaccount: Subaccount,
    pub(crate) balance: Nat,
    pub(crate) claimed_dod: u64,
    pub(crate) total_dod: u64,
    pub(crate) cycle_burning_rate: u128,
}

impl Storable for crate::types::UserDetail {
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

/// We define an example key with String
/// because String is expandable, cannot store in stable structure directly,
/// so we use a struct to wrap it.
#[derive(CandidType, Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq, Clone)]
pub struct BtreeKey(pub String);

impl Storable for BtreeKey {
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

#[derive(CandidType, Serialize, Deserialize, Clone)]
pub struct BtreeValue {
    /// key is expandable,
    /// but we have to give it a boundary
    /// say 128 bytes
    pub key: String,
    /// value is expandable,
    /// but we have to give it a boundary
    /// say 896 bytes
    pub value: Vec<u8>,
}

impl Storable for BtreeValue {
    // serialize the struct to bytes
    fn to_bytes(&self) -> Cow<[u8]> {
        candid::encode_one::<&BtreeValue>(self)
            .expect("Error: Candid Serializing BtreeValue")
            .into()
    }

    // deserialize the bytes to struct
    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        candid::decode_one::<BtreeValue>(bytes.as_ref())
            .expect("Error: Candid DeSerializing BtreeValue")
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: 64 * 2,
        is_fixed_size: false,
    };
}

// canister management

#[derive(Serialize, Deserialize, CandidType, Clone, Debug, PartialEq, Eq)]
pub struct ArchiveOptions {
    /// The number of blocks which, when exceeded, will trigger an archiving
    /// operation.
    pub trigger_threshold: usize,
    /// The number of blocks to archive when trigger threshold is exceeded.
    pub num_blocks_to_archive: usize,
    pub node_max_memory_size_bytes: Option<u64>,
    pub max_message_size_bytes: Option<u64>,
    pub controller_id: Principal,
    // More principals to add as controller of the archive.
    #[serde(default)]
    pub more_controller_ids: Option<Vec<Principal>>,
    // cycles to use for the call to create a new archive canister.
    #[serde(default)]
    pub cycles_for_archive_creation: Option<u64>,
    // Max transactions returned by the [get_transactions] endpoint.
    #[serde(default)]
    pub max_transactions_per_response: Option<u64>,
}

#[derive(Deserialize, CandidType, Clone, Debug, PartialEq, Eq)]
pub struct InitArgs {
    pub minting_account: Account,
    pub fee_collector_account: Option<Account>,
    pub initial_balances: Vec<(Account, Nat)>,
    pub transfer_fee: Nat,
    pub decimals: Option<u8>,
    pub token_name: String,
    pub token_symbol: String,
    pub metadata: Vec<(String, MetadataValue)>,
    pub archive_options: ArchiveOptions,
    pub max_memo_length: Option<u16>,
    pub feature_flags: Option<FeatureFlags>,
    pub maximum_number_of_accounts: Option<u64>,
    pub accounts_overflow_trim_quantity: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Hash, PartialEq, Serialize)]
pub struct FeeCollector<Account> {
    pub fee_collector: Account,
    /// The block index of the block where the fee_collector has
    /// been written.
    pub block_index: Option<u64>,
}

#[derive(Deserialize, CandidType, Clone, Debug, PartialEq, Eq)]
pub enum ChangeFeeCollector {
    Unset,
    SetTo(Account),
}

#[derive(Deserialize, CandidType, Clone, Debug, Default, PartialEq, Eq)]
pub struct ChangeArchiveOptions {
    pub trigger_threshold: Option<usize>,
    pub num_blocks_to_archive: Option<usize>,
    pub node_max_memory_size_bytes: Option<u64>,
    pub max_message_size_bytes: Option<u64>,
    pub controller_id: Option<Principal>,
    pub more_controller_ids: Option<Vec<Principal>>,
    pub cycles_for_archive_creation: Option<u64>,
    pub max_transactions_per_response: Option<u64>,
}

#[derive(Default, Deserialize, CandidType, Clone, Debug, PartialEq, Eq)]
pub struct UpgradeArgs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Vec<(String, MetadataValue)>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_symbol: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transfer_fee: Option<Nat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change_fee_collector: Option<ChangeFeeCollector>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_memo_length: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feature_flags: Option<FeatureFlags>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_number_of_accounts: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accounts_overflow_trim_quantity: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change_archive_options: Option<ChangeArchiveOptions>,
}

#[derive(Deserialize, CandidType, Clone, Debug, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum LedgerArgument {
    Init(InitArgs),
    Upgrade(Option<UpgradeArgs>),
}

#[derive(CandidType, Debug, Deserialize, Clone)]
pub enum IndexArg {
    Init(IndexInitArgs),
    Upgrade(IndexUpgradeArg),
}

#[derive(CandidType, Debug, Deserialize, Clone)]
pub struct IndexUpgradeArg {
    pub ledger_id: Option<Principal>,
}

#[derive(CandidType, Clone, Serialize, Deserialize, Debug)]
pub struct IndexInitArgs {
    pub(crate) ledger_id: Principal,
}

#[derive(CandidType, Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct FeatureFlags {
    pub icrc2: bool,
}

impl FeatureFlags {
    const fn const_default() -> Self {
        Self { icrc2: true }
    }
}

impl Default for FeatureFlags {
    fn default() -> Self {
        Self::const_default()
    }
}
