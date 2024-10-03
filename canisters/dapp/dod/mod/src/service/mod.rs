pub mod block;
pub mod config;
pub mod miner;
pub mod staker;

use crate::common::{
    CMCClient, NotifyTopUpRequest, CMC_CAN_ID, CYCLES_BURNER_FEE, CYCLES_CREATE_FEE, ICP_CAN_ID,
    ICP_FEE, MEMO_BURN_DOD, MEMO_TOP_UP_CANISTER, MEMO_TRANSFER, MIN_ICP_STAKE_E8S_U64,
};
use crate::management::{
    canister_add_controllers, canister_code_install, canister_code_reinstall, canister_main_create,
    Cycles,
};
use crate::memory::{
    BLOCKS, CANDIDATES, CONFIG, MINERS, NEW_BLOCK_ORDERS, NEW_USER_ORDERS, SIGS, STAKERS, TIMER_IDS,
};
use crate::orders::{NewBlockOrders, NewUserOrders};
use crate::state::{info_log_add, owners};
use crate::types::{
    ArchiveOptions, FeatureFlags, IndexArg, IndexInitArgs, InitArgs, LedgerArgument, UserDetail,
};
use base64::Engine;
use candid::{encode_args, CandidType, Deserialize, Encode, Nat, Principal};
use dod_utils::bitwork::{
    bitwork_from_height, bitwork_minus_one_hex, bitwork_plus_one_hex, Bitwork,
};
use dod_utils::fake_32;
use dod_utils::types::{
    BlockData, BlockDataFull, BlockRange, BlockSigs, BtcAddress, DodCanisters, HalvingSettings,
    Height, MinerBlockData, MinerCandidate, MinerCandidateExt, MinerInfo, MinerSubmitResponse,
    NewBlockOrderValue, OrderDetail, OrderStatus, UserBlockOrder, UserBlockOrderData,
};
use ic_cdk::api::call::RejectionCode;
use ic_cdk::{id, spawn};
use ic_cdk_timers::TimerId;
use ic_ledger_types::{
    transfer, AccountIdentifier, Memo, Subaccount, Timestamp, Tokens, TransferArgs, TransferError,
};
use ic_stable_structures::storable::Blob;
use icrc_ledger_types::icrc::generic_metadata_value::MetadataValue;
use icrc_ledger_types::icrc1::account::Account;
use icrc_ledger_types::icrc1::transfer::{NumTokens, TransferArg};
use serde::Serialize;
use std::cmp::Ordering;
use std::time::Duration;

#[derive(Clone, CandidType, Debug, Serialize, Deserialize)]
pub struct DodService {
    pub start_difficulty: Bitwork,
    pub block_time_interval: u64,
    pub difficulty_adjust_epoch: u64,
    pub default_rewards: u64,
    pub halving_settings: Option<HalvingSettings>,
    pub dod_block_sub_account: Vec<u8>,
    pub dod_token_canister: Option<Principal>,
    pub consider_decrease: Option<u64>,
    pub consider_increase: Option<u64>,
    pub ledger_wasm: Option<Vec<u8>>,
    pub index_wasm: Option<Vec<u8>>,
    pub archive_wasm: Option<Vec<u8>>,
    pub spv_wasm: Option<Vec<u8>>,
    pub dod_canisters: Option<DodCanisters>,
}

impl DodService {
    /// Retrieves the current `DodService` instance or creates a new one if it doesn't exist.
    ///
    /// This function first attempts to retrieve the current `DodService` instance from the global
    /// configuration. If an instance is found, it is returned. If no instance is found, a new
    /// `DodService` instance is created with the provided parameters and returned.
    ///
    /// # Arguments
    ///
    /// * `block_time_interval` - A `u64` representing the block time interval.
    /// * `difficulty_adjust_epoch` - A `u64` representing the difficulty adjustment epoch.
    /// * `default_rewards` - A `u64` representing the default rewards.
    /// * `dod_block_sub_account` - A `Vec<u8>` representing the DOD block sub-account.
    /// * `dod_token_canister` - An `Option<Principal>` representing the DOD token canister.
    /// * `start_difficulty` - An `Option<Bitwork>` representing the start difficulty.
    /// * `halving_settings` - An `Option<HalvingSettings>` representing the halving settings.
    ///
    /// # Returns
    ///
    /// * `DodService` - The current or newly created `DodService` instance.
    pub fn get_service(
        block_time_interval: u64,
        difficulty_adjust_epoch: u64,
        default_rewards: u64,
        dod_block_sub_account: Vec<u8>,
        dod_token_canister: Option<Principal>,
        start_difficulty: Option<Bitwork>,
        halving_settings: Option<HalvingSettings>,
    ) -> DodService {
        let _current_service = CONFIG.with(|config| {
            let config = config.borrow();
            config.dod_service.clone()
        });
        if let Some(service) = _current_service {
            service
        } else {
            DodService::new(
                block_time_interval,
                difficulty_adjust_epoch,
                default_rewards,
                halving_settings,
                dod_block_sub_account,
                dod_token_canister,
                start_difficulty,
            )
        }
    }

    /// Creates a new `DodService` instance and updates the global configuration.
    ///
    /// This function initializes a new `DodService` instance with the provided parameters.
    /// If the `start_difficulty` is not provided, it calculates the start difficulty based on the height and difficulty adjustment epoch.
    /// The new instance is then stored in the global configuration.
    ///
    /// # Arguments
    ///
    /// * `block_time_interval` - A `u64` representing the block time interval.
    /// * `difficulty_adjust_epoch` - A `u64` representing the difficulty adjustment epoch.
    /// * `default_rewards` - A `u64` representing the default rewards.
    /// * `halving_settings` - An `Option<HalvingSettings>` representing the halving settings.
    /// * `dod_block_sub_account` - A `Vec<u8>` representing the DOD block sub-account.
    /// * `dod_token_canister` - An `Option<Principal>` representing the DOD token canister.
    /// * `start_difficulty` - An `Option<Bitwork>` representing the start difficulty.
    ///
    /// # Returns
    ///
    /// * `DodService` - The newly created `DodService` instance.
    pub fn new(
        block_time_interval: u64,
        difficulty_adjust_epoch: u64,
        default_rewards: u64,
        halving_settings: Option<HalvingSettings>,
        dod_block_sub_account: Vec<u8>,
        dod_token_canister: Option<Principal>,
        start_difficulty: Option<Bitwork>,
    ) -> Self {
        CONFIG.with(|f| {
            let mut config = f.borrow_mut();
            let ser = DodService {
                block_time_interval,
                difficulty_adjust_epoch,
                default_rewards,
                halving_settings,
                dod_block_sub_account,
                dod_token_canister,
                start_difficulty: start_difficulty
                    .unwrap_or(bitwork_from_height(0, difficulty_adjust_epoch).unwrap()),
                consider_decrease: None,
                consider_increase: None,
                ledger_wasm: None,
                index_wasm: None,
                archive_wasm: None,
                spv_wasm: None,
                dod_canisters: None,
            };
            config.dod_service = Some(ser.clone());
            ser.clone()
        })
    }

    /// Cleans up various data structures by clearing their new entries.
    ///
    /// This function clears the new entries in the `MINERS`, `BLOCKS`, `SIGS`, and `CANDIDATES` data structures.
    /// It also stops and clears any active timers in the `TIMER_IDS` data structure.
    pub fn clean_up() {
        MINERS.with(|v| v.borrow_mut().clear_new());
        BLOCKS.with(|v| v.borrow_mut().clear_new());
        SIGS.with(|v| v.borrow_mut().clear_new());
        CANDIDATES.with(|v| v.borrow_mut().clear_new());
        STAKERS.with(|v| v.borrow_mut().clear_new());
        NEW_BLOCK_ORDERS.with(|v| v.borrow_mut().clear_new());
        NEW_USER_ORDERS.with(|v| v.borrow_mut().clear_new());
        TIMER_IDS.with(|v| {
            if let Some(timer_id) = v.borrow_mut().pop() {
                ic_cdk::println!("Timer canister: Stopping timer ID {timer_id:?}...");
                // It's safe to clear non-existent timer IDs.
                ic_cdk_timers::clear_timer(timer_id);
            }
            v.borrow_mut().clear()
        });
    }

    /// Retrieves the current `DodService` instance if it exists.
    ///
    /// This function accesses the global configuration to fetch the current `DodService` instance.
    /// If an instance is found, it is returned; otherwise, `None` is returned.
    ///
    /// # Returns
    ///
    /// * `Option<DodService>` - The current `DodService` instance if it exists, otherwise `None`.
    pub fn get_current_service() -> Option<DodService> {
        CONFIG.with(|config| {
            let config = config.borrow();
            config.dod_service.clone()
        })
    }

    /// Adds the ledger WASM to the service.
    ///
    /// This function sets the ledger WASM for the service and updates the service configuration.
    ///
    /// # Arguments
    ///
    /// * `ledger_wasm` - A `Vec<u8>` representing the ledger WASM.
    pub fn add_ledger_wasm(&mut self, ledger_wasm: Vec<u8>) {
        self.ledger_wasm = Some(ledger_wasm);
        self.update_self()
    }

    /// Adds the index WASM to the service.
    ///
    /// This function sets the index WASM for the service and updates the service configuration.
    ///
    /// # Arguments
    ///
    /// * `index_wasm` - A `Vec<u8>` representing the index WASM.
    pub fn add_index_wasm(&mut self, index_wasm: Vec<u8>) {
        self.index_wasm = Some(index_wasm);
        self.update_self()
    }

    /// Adds the archive WASM to the service.
    ///
    /// This function sets the archive WASM for the service and updates the service configuration.
    ///
    /// # Arguments
    ///
    /// * `archive_wasm` - A `Vec<u8>` representing the archive WASM.
    pub fn add_archive_wasm(&mut self, archive_wasm: Vec<u8>) {
        self.archive_wasm = Some(archive_wasm);
        self.update_self()
    }

    /// Updates the service configuration.
    ///
    /// This function updates the service configuration by setting the current instance of the service.
    fn update_self(&mut self) {
        CONFIG.with(|config| {
            let mut config = config.borrow_mut();
            config.dod_service = Some(self.clone());
        });
    }

    /// Sets the DOD token canister.
    ///
    /// This function updates the `dod_token_canister` field in the service configuration
    /// with the provided canister principal.
    ///
    /// # Arguments
    ///
    /// * `canister` - A `Principal` representing the new DOD token canister.
    pub fn set_token_canister(canister: Principal) {
        CONFIG.with(|config| {
            let mut config = config.borrow_mut();
            config.dod_service.as_mut().unwrap().dod_token_canister = Some(canister);
        });
    }

    /// Sets the DOD canisters in the service configuration.
    ///
    /// This function updates the `dod_canisters` field in the service configuration
    /// with the provided canister information.
    ///
    /// # Arguments
    ///
    /// * `canister` - A `DodCanisters` representing the new DOD canisters.
    pub fn set_dod_canisters(canister: DodCanisters) {
        CONFIG.with(|config| {
            let mut config = config.borrow_mut();
            config.dod_service.as_mut().unwrap().dod_canisters = Some(canister);
        });
    }

    /// Retrieves the DOD canisters from the service configuration.
    ///
    /// This function accesses the global `CONFIG` to borrow the current configuration,
    /// and then retrieves the `dod_canisters` field from the `dod_service`.
    ///
    /// # Returns
    ///
    /// * `Option<DodCanisters>` - Returns `Some(DodCanisters)` if the canisters are set, otherwise `None`.
    pub fn get_dod_canisters() -> Option<DodCanisters> {
        CONFIG.with(|config| {
            let config = config.borrow();
            config.dod_service.as_ref().unwrap().dod_canisters.clone()
        })
    }

    /// Deploys the DOD ledger canister along with its index and archive canisters.
    ///
    /// This function performs the following steps:
    /// 1. Retrieves the list of owners and the current canister ID.
    /// 2. Checks if the necessary WASM binaries (ledger, index, archive) are available.
    /// 3. Creates the ledger, index, and archive canisters.
    /// 4. Installs the respective WASM binaries on the created canisters.
    /// 5. Adds the necessary controllers to the canisters.
    ///
    /// # Returns
    ///
    /// * `Result<Principal, String>` - On success, returns the principal ID of the ledger canister. On failure, returns an error message.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// * Any of the WASM binaries are not found.
    /// * There is an error creating any of the canisters.
    /// * There is an error installing the WASM binaries on the canisters.
    /// * There is an error adding controllers to the canisters.
    pub async fn deploy_dod_ledger(&self) -> Result<Principal, String> {
        let _owners = owners().map_or(vec![], |v| {
            v.iter().map(|v| v.0.clone()).collect::<Vec<Principal>>()
        });
        let dod_canister = id();
        let mut all_owners = vec![dod_canister.clone()];
        all_owners.extend_from_slice(_owners.clone().as_slice());

        if self.ledger_wasm.is_none() {
            return Err("Ledger wasm not found".to_string());
        }
        if self.index_wasm.is_none() {
            return Err("Index wasm not found".to_string());
        }
        if self.archive_wasm.is_none() {
            return Err("Archive wasm not found".to_string());
        }

        let fee = CYCLES_CREATE_FEE;

        let leger_canister_id = canister_main_create(Cycles::from(fee)).await.map_err(|e| {
            println!("Error create ledger canister: {:?}", e.msg);
            e.msg
        })?;
        println!("ledger canister: {:?} created", leger_canister_id);

        let index_canister_id = canister_main_create(Cycles::from(fee)).await.map_err(|e| {
            println!("Error create index canister: {:?}", e.msg);
            e.msg
        })?;
        println!("index canister: {:?} created", index_canister_id);
        let archive_canister_id = canister_main_create(Cycles::from(fee)).await.map_err(|e| {
            println!("Error create archive canister: {:?}", e.msg);
            e.msg
        })?;
        println!("archive canister: {:?} created", archive_canister_id);

        Self::set_token_canister(leger_canister_id);

        let _ledger_install_result = canister_code_install(
            leger_canister_id.clone(),
            self.ledger_wasm.clone().unwrap(),
            Encode!(&LedgerArgument::Init(InitArgs {
                minting_account: Account {
                    owner: dod_canister.clone(),
                    subaccount: None
                },
                fee_collector_account: None,
                initial_balances: vec![],
                transfer_fee: Nat::from(0u64),
                decimals: Some(8),
                token_name: "DOD".to_string(),
                token_symbol: "𓃡𓃡𓃡".to_string(),
                metadata: vec![(
                    "content-type".to_string(),
                    MetadataValue::from("application/json")
                ),],
                archive_options: ArchiveOptions {
                    trigger_threshold: 1000,
                    num_blocks_to_archive: 2000,
                    node_max_memory_size_bytes: None,
                    max_message_size_bytes: None,
                    controller_id: dod_canister.clone(),
                    more_controller_ids: None,
                    cycles_for_archive_creation: None,
                    max_transactions_per_response: None,
                },
                max_memo_length: Some(512),
                feature_flags: Some(FeatureFlags { icrc2: true }),
                maximum_number_of_accounts: None,
                accounts_overflow_trim_quantity: None,
            }))
            .ok(),
        )
        .await
        .map_err(|e| {
            println!("Error installing ledger canister: {:?}", e.msg);
            e.msg
        })?;

        let _index_install_result = canister_code_install(
            index_canister_id.clone(),
            self.index_wasm.clone().unwrap(),
            Encode!(&Some(IndexArg::Init(IndexInitArgs {
                ledger_id: leger_canister_id.clone()
            })))
            .ok(),
        )
        .await
        .map_err(|e| {
            println!("Error installing index canister: {:?}", e.msg);
            e.msg
        })?;

        let _archive_install_result = canister_code_install(
            archive_canister_id.clone(),
            self.archive_wasm.clone().unwrap(),
            encode_args((leger_canister_id.clone(), 2000u64, None::<u64>, None::<u64>)).ok(),
        )
        .await
        .map_err(|e| {
            println!("Error installing archive canister: {:?}", e.msg);
            e.msg
        })?;

        canister_add_controllers(leger_canister_id.clone(), all_owners.clone())
            .await
            .map_err(|e| {
                println!("Error add controller to ledger canister: {:?}", e.msg);
                e.msg
            })?;
        canister_add_controllers(index_canister_id.clone(), all_owners.clone())
            .await
            .map_err(|e| {
                println!("Error add controller to index canister: {:?}", e.msg);
                e.msg
            })?;
        canister_add_controllers(archive_canister_id.clone(), all_owners.clone())
            .await
            .map_err(|e| {
                println!("Error add controller to archive canister: {:?}", e.msg);
                e.msg
            })?;

        Self::set_dod_canisters(DodCanisters {
            ledger: leger_canister_id,
            index: index_canister_id,
            archive: archive_canister_id,
        });

        Ok(leger_canister_id.clone())
    }

    pub async fn reset_ledgers(&self) -> Result<(), String> {
        let leger_canister_id = Self::get_dod_canisters().unwrap().ledger;
        let index_canister_id = Self::get_dod_canisters().unwrap().index;
        let archive_canister_id = Self::get_dod_canisters().unwrap().archive;
        let dod_canister = id();

        let _ledger_install_result = canister_code_reinstall(
            leger_canister_id.clone(),
            self.ledger_wasm.clone().unwrap(),
            Encode!(&LedgerArgument::Init(InitArgs {
                minting_account: Account {
                    owner: dod_canister.clone(),
                    subaccount: None
                },
                fee_collector_account: None,
                initial_balances: vec![],
                transfer_fee: Nat::from(0u64),
                decimals: Some(8),
                token_name: "DOD".to_string(),
                token_symbol: "𓃡𓃡𓃡".to_string(),
                metadata: vec![(
                    "content-type".to_string(),
                    MetadataValue::from("application/json")
                ),],
                archive_options: ArchiveOptions {
                    trigger_threshold: 1000,
                    num_blocks_to_archive: 2000,
                    node_max_memory_size_bytes: None,
                    max_message_size_bytes: None,
                    controller_id: dod_canister.clone(),
                    more_controller_ids: None,
                    cycles_for_archive_creation: None,
                    max_transactions_per_response: None,
                },
                max_memo_length: Some(512),
                feature_flags: Some(FeatureFlags { icrc2: true }),
                maximum_number_of_accounts: None,
                accounts_overflow_trim_quantity: None,
            }))
            .ok(),
        )
        .await
        .map_err(|e| {
            println!("Error installing ledger canister: {:?}", e.msg);
            e.msg
        })?;

        let _index_install_result = canister_code_reinstall(
            index_canister_id.clone(),
            self.index_wasm.clone().unwrap(),
            Encode!(&Some(IndexArg::Init(IndexInitArgs {
                ledger_id: leger_canister_id.clone()
            })))
            .ok(),
        )
        .await
        .map_err(|e| {
            println!("Error installing index canister: {:?}", e.msg);
            e.msg
        })?;

        let _archive_install_result = canister_code_reinstall(
            archive_canister_id.clone(),
            self.archive_wasm.clone().unwrap(),
            encode_args((leger_canister_id.clone(), 2000u64, None::<u64>, None::<u64>)).ok(),
        )
        .await
        .map_err(|e| {
            println!("Error installing archive canister: {:?}", e.msg);
            e.msg
        })?;

        Ok(())
    }

    /// Retrieves the token canister.
    ///
    /// # Returns
    ///
    /// * `Result<Principal, String>` - On success, returns the `Principal` of the token canister. On failure, returns an error message as a `String`.
    pub fn get_token_canister() -> Result<Principal, String> {
        config::get_token_canister()
    }

    /// Retrieves the DOD block account.
    ///
    /// This function calls the `get_dod_block_account` function from the `config` module
    /// to obtain the DOD block account.
    ///
    /// # Returns
    ///
    /// * `Result<Account, String>` - On success, returns the `Account` of the DOD block. On failure, returns an error message as a `String`.
    pub fn get_dod_block_account() -> Result<[u8; 32], String> {
        config::get_dod_block_account()
    }

    /// Retrieves the block time interval.
    ///
    /// # Returns
    ///
    /// * `Result<u64, String>` - On success, returns the block time interval as `u64`. On failure, returns an error message as a `String`.
    pub fn get_block_time_interval() -> Result<u64, String> {
        config::get_block_time_interval()
    }

    /// Retrieves the difficulty adjustment epoch.
    ///
    /// # Returns
    ///
    /// * `Result<u64, String>` - On success, returns the difficulty adjustment epoch as `u64`. On failure, returns an error message as a `String`.
    pub fn get_difficulty_adjust_epoch() -> Result<u64, String> {
        config::get_difficulty_adjust_epoch()
    }

    /// Retrieves the default rewards.
    ///
    /// # Returns
    ///
    /// * `Result<u64, String>` - On success, returns the default rewards as `u64`. On failure, returns an error message as a `String`.
    pub fn get_default_rewards() -> Result<u64, String> {
        config::get_default_rewards()
    }

    /// Retrieves the start difficulty.
    ///
    /// # Returns
    ///
    /// * `Result<Bitwork, String>` - On success, returns the start difficulty as `Bitwork`. On failure, returns an error message as a `String`.
    pub fn get_start_difficulty() -> Result<Bitwork, String> {
        config::get_start_difficulty()
    }

    /// Retrieves the halving settings.
    ///
    /// This function calls the `get_halving_settings` function from the `config` module
    /// to obtain the halving settings.
    ///
    /// # Returns
    ///
    /// * `Result<HalvingSettings, String>` - On success, returns the `HalvingSettings`.
    ///   On failure, returns an error message as a `String`.
    pub fn get_halving_settings() -> Option<HalvingSettings> {
        config::get_halving_settings()
    }

    /// Retrieves the consider decrease value.
    ///
    /// # Returns
    ///
    /// * `Result<Option<u64>, String>` - On success, returns the consider decrease value as `Option<u64>`. On failure, returns an error message as a `String`.
    pub fn get_consider_decrease() -> Result<Option<u64>, String> {
        config::get_consider_decrease()
    }

    /// Retrieves the consider increase value.
    ///
    /// # Returns
    ///
    /// * `Result<Option<u64>, String>` - On success, returns the consider increase value as `Option<u64>`. On failure, returns an error message as a `String`.
    pub fn get_consider_increase() -> Result<Option<u64>, String> {
        config::get_consider_increase()
    }

    /// Sets the consider decrease value.
    ///
    /// # Arguments
    ///
    /// * `consider_decrease` - An `Option<u64>` representing the consider decrease value to be set.
    ///
    /// # Returns
    ///
    /// * `Result<(), String>` - On success, returns `Ok(())`. On failure, returns an error message as a `String`.
    pub fn set_consider_decrease(consider_decrease: Option<u64>) -> Result<(), String> {
        config::set_consider_decrease(consider_decrease)
    }

    /// Sets the consider increase value.
    ///
    /// # Arguments
    ///
    /// * `consider_increase` - An `Option<u64>` representing the consider increase value to be set.
    ///
    /// # Returns
    ///
    /// * `Result<(), String>` - On success, returns `Ok(())`. On failure, returns an error message as a `String`.
    pub fn set_consider_increase(consider_increase: Option<u64>) -> Result<(), String> {
        config::set_consider_increase(consider_increase)
    }

    // Staker Execution
    /// Generates a subaccount from a given `Principal` identifier.
    ///
    /// # Arguments
    ///
    /// * `id` - A `Principal` representing the identifier from which the subaccount is to be generated.
    ///
    /// # Returns
    ///
    /// * `Subaccount` - The generated subaccount.
    pub fn user_subaccount(id: Principal) -> Subaccount {
        Subaccount::from(id)
    }

    /// Registers a user.
    ///
    /// # Arguments
    ///
    /// * `user` - A `Principal` representing the user to be registered.
    ///
    /// # Returns
    ///
    /// * `Result<(), String>` - On success, returns `Ok(())`. On failure, returns an error message as a `String`.
    pub fn register_user(user: Principal) -> Result<(), String> {
        staker::register_user(user)
    }

    /// Sets the burn rate for a given user.
    ///
    /// # Arguments
    ///
    /// * `user` - A `Principal` representing the user whose burn rate is to be set.
    /// * `burn_rate` - A `u128` value representing the new burn rate to be set for the user.
    ///
    /// # Returns
    ///
    /// * `Result<(), String>` - On success, returns `Ok(())`. On failure, returns an error message as a `String`.
    pub fn user_set_burnrate(user: Principal, burn_rate: u128) -> Result<(), String> {
        staker::user_set_burnrate(user, burn_rate)
    }

    /// Retrieves the burn rate and balance for a given user.
    ///
    /// # Arguments
    ///
    /// * `user` - A `Principal` representing the user whose burn rate and balance are to be retrieved.
    ///
    /// # Returns
    ///
    /// * `Result<(u128, Nat), String>` - On success, returns a tuple containing the burn rate as `u128` and the balance as `Nat`.
    ///   On failure, returns an error message as a `String`.
    pub fn get_user_burnrate(user: Principal) -> Result<(u128, Nat), String> {
        staker::get_user_burnrate(user)
    }

    /// Places burn rate orders for a user.
    ///
    /// This function calculates the number of orders based on the user's burn rate and the specified burn amount.
    /// It then places the orders for the user within the specified block range.
    ///
    /// # Arguments
    ///
    /// * `user` - A `Principal` representing the user.
    /// * `start_height` - A `Height` representing the starting block height.
    /// * `burn_amount` - A `u128` representing the total amount to be burned.
    ///
    /// # Returns
    ///
    /// * `Result<(), String>` - Returns `Ok(())` if the orders are successfully placed, otherwise returns an error message.
    pub fn user_put_burnrate_orders(
        user: Principal,
        start_height: Height,
        burn_amount: u128,
    ) -> Result<(), String> {
        match Self::get_user_burnrate(user) {
            Ok((rate, balance)) => {
                let n_rate = Nat::from(rate);
                let n_amount = Nat::from(burn_amount);

                if balance < n_amount {
                    return Err("Not enough balance".to_string());
                }

                if balance < n_rate {
                    return Err("Not enough balance".to_string());
                }

                ic_cdk::println!("Burn rate: {:?}, Burn amount: {:?}", rate, burn_amount);

                let times: u128 =
                    u128::try_from((burn_amount / n_rate).0).expect("Can not convert to u128");

                if times == 0 {
                    return Err("Amount too low".to_string());
                }

                // if times > BURN_ORDERS_LIMIT {
                //     return Err(format!(
                //         "Burn Orders are over the limit {:?}",
                //         BURN_ORDERS_LIMIT
                //     ));
                // }

                // for i in 0..times {
                //     Self::user_put_order(
                //         user.clone(),
                //         UserType::User,
                //         start_height + u64::try_from(i).expect("can not convert to u64"),
                //         rate,
                //     )
                //     .expect("Can not put order");
                // }

                let end_height =
                    start_height + u64::try_from(times).expect("can not convert to u64");

                Self::user_put_order_v2(user.clone(), (start_height, end_height), rate);

                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// Retrieves the current number of miners.
    ///
    /// # Returns
    ///
    /// * `u32` - The current number of miners.
    pub fn get_current_miners_length() -> u32 {
        miner::get_current_miners_length()
    }

    /// Checks if a miner exists.
    ///
    /// # Arguments
    ///
    /// * `caller` - A `Principal` representing the caller.
    /// * `btc_address` - A `String` representing the Bitcoin address.
    ///
    /// # Returns
    ///
    /// * `Option<MinerInfo>` - Returns `Some(MinerInfo)` if the miner exists, otherwise `None`.
    pub fn check_miner_if_existed(caller: Principal, btc_address: String) -> Option<MinerInfo> {
        miner::check_miner_if_existed(caller, btc_address)
    }

    /// Loads signatures by block height.
    ///
    /// # Arguments
    ///
    /// * `height` - A `Height` representing the block height.
    ///
    /// # Returns
    ///
    /// * `Option<BlockSigs>` - Returns `Some(BlockSigs)` if signatures are found, otherwise `None`.
    pub fn load_sigs_by_height(height: Height) -> Option<BlockSigs> {
        miner::load_sigs_by_height(height)
    }

    /// Submits hashes for a miner.
    ///
    /// # Arguments
    ///
    /// * `caller` - A `Principal` representing the caller.
    /// * `btc_address` - A `String` representing the Bitcoin address.
    /// * `signed_commit_psbt` - A `String` representing the signed commit PSBT.
    /// * `signed_reveal_psbt` - A `String` representing the signed reveal PSBT.
    /// * `cycles_price` - A `u128` representing the cycles price.
    ///
    /// # Returns
    ///
    /// * `Result<MinerSubmitResponse, String>` - On success, returns `MinerSubmitResponse`. On failure, returns an error message as a `String`.
    pub fn miner_submit_hashes(
        caller: Principal,
        btc_address: String,
        signed_commit_psbt: String,
        signed_reveal_psbt: String,
        cycles_price: u128,
    ) -> Result<MinerSubmitResponse, String> {
        miner::miner_submit_hashes(
            caller,
            btc_address,
            signed_commit_psbt,
            signed_reveal_psbt,
            cycles_price,
        )
    }

    /// Adds a block candidate.
    ///
    /// # Arguments
    ///
    /// * `height` - A `Height` representing the block height.
    /// * `miner_candidate` - A `MinerCandidate` representing the miner candidate.
    pub fn add_block_candidate(height: Height, miner_candidate: MinerCandidate) {
        miner::add_block_candidate(height, miner_candidate)
    }

    /// Retrieves block candidates by height.
    ///
    /// # Arguments
    ///
    /// * `height` - A `Height` representing the block height.
    ///
    /// # Returns
    ///
    /// * `Vec<MinerCandidate>` - A vector of `MinerCandidate` for the given height.
    pub fn get_block_candidates(height: Height) -> Vec<MinerCandidate> {
        miner::get_block_candidates(height)
    }

    /// Checks if a Bitcoin address is in the candidate list for a given block.
    ///
    /// # Arguments
    ///
    /// * `btc_address` - A `String` representing the Bitcoin address.
    /// * `block` - A `Height` representing the block height.
    ///
    /// # Returns
    ///
    /// * `Option<MinerCandidate>` - Returns `Some(MinerCandidate)` if the address is in the candidate list, otherwise `None`.
    pub fn check_if_in_candidate(btc_address: String, block: Height) -> Option<MinerCandidate> {
        miner::check_if_in_candidate(btc_address, block)
    }

    /// Retrieves miner information by principal.
    ///
    /// # Arguments
    ///
    /// * `principal` - A `Principal` representing the principal.
    ///
    /// # Returns
    ///
    /// * `Option<MinerInfo>` - Returns `Some(MinerInfo)` if the miner is found, otherwise `None`.
    pub fn get_miner_by_principal(principal: Principal) -> Option<MinerInfo> {
        miner::get_miner_by_principal(principal)
    }

    /// Registers a miner.
    ///
    /// # Arguments
    ///
    /// * `owner` - A `Principal` representing the owner.
    /// * `btc_address` - A `String` representing the Bitcoin address.
    /// * `ecdsa_pubkey` - A `Vec<u8>` representing the ECDSA public key.
    ///
    /// # Returns
    ///
    /// * `Result<MinerInfo, String>` - On success, returns `MinerInfo`. On failure, returns an error message as a `String`.
    pub fn register_miner(
        owner: Principal,
        btc_address: String,
        ecdsa_pubkey: Vec<u8>,
    ) -> Result<MinerInfo, String> {
        miner::register_miner(owner, btc_address, ecdsa_pubkey)
    }

    /// Retrieves miner information by address.
    ///
    /// # Arguments
    ///
    /// * `address` - A `String` representing the address.
    ///
    /// # Returns
    ///
    /// * `Option<MinerInfo>` - Returns `Some(MinerInfo)` if the miner is found, otherwise `None`.
    pub fn get_miner_by_address(address: String) -> Option<MinerInfo> {
        miner::get_miner_by_address(address)
    }

    /// Retrieves the mining history for a given Bitcoin address within a specified block range.
    ///
    /// This function calls the `get_mining_history_for_miners` function from the `miner` module
    /// to obtain the mining history for the specified Bitcoin address and block range.
    ///
    /// # Arguments
    ///
    /// * `btc_address` - A `String` representing the Bitcoin address.
    /// * `block_range` - A `BlockRange` representing the range of blocks to retrieve the mining history for.
    ///
    /// # Returns
    ///
    /// * `Vec<MinerBlockData>` - A vector of `MinerBlockData` containing the mining history for the specified address and block range.
    pub fn get_mining_history_for_miners(
        btc_address: String,
        block_range: BlockRange,
    ) -> Vec<MinerBlockData> {
        miner::get_mining_history_for_miners(btc_address, block_range)
    }

    //  Blocks Execution

    /// Retrieves all blocks.
    ///
    /// # Returns
    ///
    /// * `Vec<BlockData>` - A vector of `BlockData` representing all blocks.
    pub fn get_blocks() -> Vec<BlockData> {
        block::get_blocks()
    }

    /// Retrieves blocks within a specified range.
    ///
    /// # Arguments
    ///
    /// * `from` - A `Height` representing the starting height.
    /// * `to` - A `Height` representing the ending height.
    ///
    /// # Returns
    ///
    /// * `Vec<BlockData>` - A vector of `BlockData` representing the blocks within the specified range.
    pub fn get_blocks_range(from: Height, to: Height) -> Vec<BlockData> {
        block::get_blocks_range(from, to)
    }

    /// Retrieves the count of failed blocks in the last epoch.
    ///
    /// # Arguments
    ///
    /// * `start_height` - A `Height` representing the starting height.
    ///
    /// # Returns
    ///
    /// * `(u64, u64, f64)` - A tuple containing the count of failed blocks, the total number of blocks, and the failure rate.
    pub fn get_last_epoch_failed_blocks_count(start_height: Height) -> (u64, u64, f64) {
        block::get_last_epoch_failed_blocks_count(start_height)
    }

    /// Starts the process of generating blocks asynchronously.
    ///
    /// This function initiates the block generation process and sets a timer to
    /// periodically generate blocks based on the block time interval.
    ///
    /// # Returns
    ///
    /// * `Result<(), String>` - On success, returns `Ok(())`. On failure, returns an error message as a `String`.
    pub async fn start_generate_blocks() -> Result<(), String> {
        Self::generate_blocks();
        let block_time_interval = Self::get_block_time_interval()?;
        Self::set_timer(block_time_interval, Self::generate_blocks);
        Ok(())
    }

    /// Sets a timer to execute a callback function at a specified interval.
    ///
    /// # Arguments
    ///
    /// * `interval` - A `u64` representing the interval in nanoseconds.
    /// * `callback` - A function pointer to the callback function to be executed.
    ///
    /// # Returns
    ///
    /// * `TimerId` - The ID of the created timer.
    pub fn set_timer(interval: u64, callback: fn()) -> TimerId {
        let secs = Duration::from_nanos(interval);
        ic_cdk::println!("Timer canister: Starting a new timer with {secs:?} interval...");
        // Schedule a new periodic task to increment the counter.
        let timer_id = ic_cdk_timers::set_timer_interval(secs, callback);

        // Add the timer ID to the global vector.
        TIMER_IDS.with(|timer_ids| timer_ids.borrow_mut().push(timer_id));
        timer_id
    }

    pub fn timer_stop() {
        TIMER_IDS.with(|timer_ids| {
            if let Some(timer_id) = timer_ids.borrow_mut().pop() {
                ic_cdk::println!("Timer canister: Stopping timer ID {timer_id:?}...");
                // It's safe to clear non-existent timer IDs.
                ic_cdk_timers::clear_timer(timer_id);
            }
        });
    }

    pub fn set_timer_delay(interval: u64, callback: fn()) -> TimerId {
        let secs = Duration::from_nanos(interval);
        ic_cdk::println!("Timer canister: Starting a new timer with {secs:?} interval...");
        // Schedule a new periodic task to increment the counter.
        let timer_id = ic_cdk_timers::set_timer(secs, callback);

        // Add the timer ID to the global vector.
        TIMER_IDS.with(|timer_ids| timer_ids.borrow_mut().push(timer_id));
        timer_id
    }

    pub fn generate_blocks() {
        let block_time_interval = Self::get_block_time_interval().unwrap();
        let difficulty_adjust_epoch = Self::get_difficulty_adjust_epoch().unwrap();
        let default_rewards = Self::get_default_rewards().unwrap();
        let start_difficulty = Self::get_start_difficulty().unwrap();
        let halving_settings = Self::get_halving_settings();
        match Self::get_last_block() {
            None => {
                let mut random_32 = fake_32();
                random_32.reverse();
                // genesis block
                let time = ic_cdk::api::time();
                let bitwork = start_difficulty.clone();

                Self::set_consider_increase(Some(0 + difficulty_adjust_epoch))
                    .expect("Can not set consider increase height");

                let block_data = BlockData {
                    height: 0,
                    rewards: default_rewards,
                    winner: None,
                    difficulty: bitwork,
                    hash: random_32,
                    block_time: time,
                    next_block_time: time + block_time_interval,
                    history: false,
                    cycle_burned: 0,
                    dod_burned: 0,
                };
                BLOCKS.with(|v| v.borrow_mut().insert(0, block_data.clone()));

                // Ok(block_data.clone());
            }
            Some(r) => {
                Self::timer_stop();

                let last_block = r.1;

                let last_block_reward =
                    Self::get_block_reward_by_height(last_block.height, halving_settings).unwrap();

                // temporally comment out the burn DOD from treasury
                spawn(async move {
                    let _ = Self::mint_dod_award_to_treasury(last_block_reward).await;
                    //.expect("Can not mint DOD award to treasury");
                });

                // 1. handle candidates sorting, price lowest first, submit time first
                let mut candidates = Self::get_block_candidates(last_block.height);
                candidates.sort();
                let winner_address = if candidates.len() > 0 {
                    Some(candidates[0].btc_address.clone())
                } else {
                    None
                };
                let cycle_price = if candidates.len() > 0 {
                    Some(candidates[0].cycles_price.clone())
                } else {
                    None
                };

                // 1.1 should get current block total cycles to see the price if winner can win.
                let cycle_deposit = Self::get_block_total_cycles(last_block.height, false);

                ic_cdk::println!("cycle_deposit is {:?}", cycle_deposit);

                let mut _miner = None;
                #[allow(unused_assignments)]
                let mut treasury_revinvest = 0u128;
                #[allow(unused_assignments)]
                let mut to_burn = 0u128;

                if winner_address.is_some()
                    && cycle_price.is_some()
                    && cycle_deposit > cycle_price.unwrap()
                {
                    let miner_info = Self::get_miner_by_address(winner_address.unwrap()).unwrap();
                    _miner = Some(MinerInfo {
                        reward_cycles: Some(cycle_price.unwrap()),
                        ..miner_info.clone()
                    });

                    treasury_revinvest = (cycle_deposit - cycle_price.unwrap()) / 2;

                    // because we have miner meanwhile owner as staker,
                    // we increase the balance from cycle price for miners
                    Self::increase_user_cycle_balance(
                        miner_info.owner.clone(),
                        Nat::from(cycle_price.unwrap()),
                    )
                    .unwrap();
                } else {
                    treasury_revinvest = cycle_deposit / 2;
                }

                // to burn equals to treasury_revinvest

                to_burn = treasury_revinvest.clone();
                Self::user_put_order_v2(
                    id(),
                    (last_block.height + 1, last_block.height + 2),
                    treasury_revinvest,
                );

                // 2. write block data and update winner to storage

                let mut _block = last_block.clone();

                _block.winner = _miner.clone();
                _block.history = true;

                // 3. write winner sigs to storage
                if _block.winner.is_some() {
                    let commit_tx = base64::engine::general_purpose::STANDARD
                        .decode(candidates[0].signed_commit_psbt.clone())
                        .map_err(|_| "can not decode base64".to_string())
                        .unwrap();
                    let reveal_tx = base64::engine::general_purpose::STANDARD
                        .decode(candidates[0].signed_reveal_psbt.clone())
                        .map_err(|_| "can not decode base64".to_string())
                        .unwrap();

                    SIGS.with(|v| {
                        v.borrow_mut().insert(
                            _block.height.clone(),
                            BlockSigs {
                                commit_tx,
                                reveal_tx,
                            },
                        )
                    });
                }

                // 3.3 update all user balances

                Self::update_users_balance_v2(last_block.height, cycle_deposit);

                // 4. burn  cycles here
                ic_cdk::println!(
                    "{}",
                    format!(
                        "execute_cycles_on_block_data, cycle_deposit is {:?}, expect to burn {:?}",
                        cycle_deposit, to_burn
                    )
                );

                info_log_add(
                    format!(
                        "execute_cycles_on_block_data, cycle_deposit is {:?}, expect to burn {:?}",
                        cycle_deposit, to_burn
                    )
                    .as_str(),
                );

                // temporally comment out execute_cycles_on_block_data
                Self::execute_cycles_on_block_data(to_burn.clone()).unwrap();

                // 4.1 burn DOD

                _block.cycle_burned = to_burn.clone();
                ic_cdk::println!("block.winner is {:?}", _miner.clone());
                let _id = id();
                let (total_burn, _) = Self::get_user_block_reward(_block.height.clone(), _id);
                ic_cdk::println!("dod total burn is {:?}", total_burn);

                if total_burn == Self::get_default_rewards().unwrap() {
                    ic_cdk::println!("No one deposit cycles in this block, we should stop here");
                    return;
                }

                // temporally comment out the burn DOD from treasury
                spawn(async move {
                    let _ = Self::burn_dod_from_treasury(_id, total_burn).await;
                    // .expect("Can not burn DOD from treasury");
                });

                _block.dod_burned = total_burn.clone();
                BLOCKS.with(|v| v.borrow_mut().insert(_block.height.clone(), _block.clone()));

                // 5. create new block
                let mut random_32 = fake_32();
                random_32.reverse();

                // 6. difficulty adjust
                let mut bitwork;

                bitwork = last_block.difficulty.clone();

                if _block.winner.is_none() {
                    let considered = Self::get_consider_decrease().unwrap();

                    match considered {
                        None => {
                            Self::set_consider_decrease(Some(
                                _block.height + difficulty_adjust_epoch,
                            ))
                            .expect("Can not set consider decrease height");

                            Self::set_consider_increase(None)
                                .expect("Can not set consider increase height");
                        }
                        Some(i) => {
                            if _block.height + 1 == i {
                                let decreased =
                                    bitwork_minus_one_hex(last_block.difficulty.clone()).unwrap();

                                if decreased.cmp(&start_difficulty) == Ordering::Less {
                                    bitwork = start_difficulty.clone();
                                } else {
                                    bitwork = decreased;
                                }

                                Self::set_consider_decrease(Some(i + difficulty_adjust_epoch))
                                    .expect("Can not set consider decrease height");
                            }
                        }
                    }
                } else {
                    let considered = Self::get_consider_increase().unwrap();
                    match considered {
                        None => {
                            Self::set_consider_increase(Some(
                                _block.height + difficulty_adjust_epoch,
                            ))
                            .expect("Can not set consider increase height");

                            Self::set_consider_decrease(None)
                                .expect("Can not set consider decrease height");
                        }
                        Some(i) => {
                            if _block.height + 1 == i {
                                bitwork =
                                    bitwork_plus_one_hex(last_block.difficulty.clone()).unwrap();
                                Self::set_consider_increase(Some(i + difficulty_adjust_epoch))
                                    .expect("Can not set consider increase height");
                            }
                        }
                    }
                }

                let current_time = ic_cdk::api::time();
                let block_data = BlockData {
                    height: last_block.height + 1,
                    rewards: default_rewards,
                    winner: None,
                    difficulty: bitwork,
                    hash: random_32,
                    block_time: current_time,
                    next_block_time: current_time + block_time_interval,
                    history: false,
                    cycle_burned: 0,
                    dod_burned: 0,
                };
                BLOCKS.with(|v| v.borrow_mut().insert(block_data.height, block_data.clone()));
                Self::set_timer_delay(block_time_interval, Self::generate_blocks);
                // Ok(block_data.clone());
            }
        }
    }

    /// Retrieves the last block.
    ///
    /// # Returns
    ///
    /// * `Option<(u64, BlockData)>` - Returns `Some((u64, BlockData))` if the last block exists, otherwise `None`.
    pub fn get_last_block() -> Option<(u64, BlockData)> {
        block::get_last_block()
    }

    /// Retrieves a block by its height.
    ///
    /// # Arguments
    ///
    /// * `height` - A `u64` representing the height of the block.
    ///
    /// # Returns
    ///
    /// * `Option<BlockData>` - Returns `Some(BlockData)` if the block exists, otherwise `None`.
    pub fn get_block_by_height(height: u64) -> Option<BlockData> {
        block::get_block_by_height(height)
    }

    /// Deposits cycles from ICP.
    ///
    /// This function transfers ICP to the CMC canister and notifies the top-up, converting the ICP to cycles.
    ///
    /// # Arguments
    ///
    /// * `from` - A `Principal` representing the sender.
    /// * `qty_e8s_u64` - A `u64` representing the quantity of ICP in e8s.
    ///
    /// # Panics
    ///
    /// This function will panic if the quantity of ICP is less than the minimum required stake.
    ///
    /// # Steps
    ///
    /// 1. Transfers ICP to the CMC canister.
    /// 2. Notifies the top-up to convert ICP to cycles.
    /// 3. Updates the user's balance with the new cycles.
    pub async fn deposit_cycles_from_icp(from: Principal, qty_e8s_u64: u64) {
        if qty_e8s_u64 < MIN_ICP_STAKE_E8S_U64 {
            panic!(
                "At least 0.5 ICP is required to fuel the furnace, but got {}",
                qty_e8s_u64
            );
        }
        let caller_subaccount = Subaccount::from(from.clone());
        let icp_can_id = Principal::from_text(ICP_CAN_ID).unwrap();
        let cmc_can_id = Principal::from_text(CMC_CAN_ID).unwrap();
        let canister_id = id();
        let subaccount = Subaccount::from(canister_id);

        let transfer_args = TransferArgs {
            amount: Tokens::from_e8s(qty_e8s_u64),
            to: AccountIdentifier::new(&cmc_can_id, &subaccount),
            memo: Memo(MEMO_TOP_UP_CANISTER),
            fee: Tokens::from_e8s(ICP_FEE),
            from_subaccount: Some(caller_subaccount),
            created_at_time: Some(Timestamp {
                timestamp_nanos: ic_cdk::api::time(),
            }),
        };

        let block_index = transfer(icp_can_id, transfer_args)
            .await
            .expect("Unable to call ICP canister")
            .expect("Unable to transfer ICP");

        let cmc = CMCClient(cmc_can_id);

        let notify_args = NotifyTopUpRequest {
            block_index,
            canister_id,
        };

        let cycles = cmc
            .notify_top_up(notify_args)
            .await
            .expect("Unable to call cycle canister")
            .0
            .expect("Unable to deposit cycles");

        let blob29 = Blob::<29>::try_from(from.clone().as_slice()).expect("error transformation");
        let user = Self::get_user_detail(from.clone());

        if user.is_some() {
            let user = user.unwrap();

            STAKERS.with(|v| {
                v.borrow_mut().insert(
                    blob29,
                    UserDetail {
                        balance: user.balance + cycles,
                        ..user
                    },
                );
            })
        } else {
            STAKERS.with(|v| {
                v.borrow_mut().insert(
                    blob29,
                    UserDetail {
                        principal: from.clone(),
                        subaccount,
                        balance: cycles,
                        claimed_dod: 0,
                        total_dod: 0,
                        cycle_burning_rate: 0,
                    },
                );
            })
        }
    }

    /// Retrieves the details of a user.
    ///
    /// # Arguments
    ///
    /// * `user` - A `Principal` representing the user.
    ///
    /// # Returns
    ///
    /// * `Option<UserDetail>` - Returns `Some(UserDetail)` if the user exists, otherwise `None`.
    pub fn get_user_detail(user: Principal) -> Option<UserDetail> {
        let blob29 = Blob::<29>::try_from(user.as_slice()).expect("error transformation");
        STAKERS.with(|v| v.borrow().get(&blob29).map(|v| v.clone()))
    }
    /// Writes the cycle balance for a user.
    ///
    /// # Arguments
    ///
    /// * `user` - A `Principal` representing the user.
    /// * `balance` - A `Nat` representing the new balance to be set.
    ///
    /// # Returns
    ///
    /// * `Result<(), String>` - On success, returns `Ok(())`. On failure, returns an error message as a `String`.
    pub fn increase_user_cycle_balance(
        user: Principal,
        increase_balance: Nat,
    ) -> Result<(), String> {
        match Self::get_user_detail(user) {
            None => Err("No user found".to_string()),
            Some(r) => {
                let blob29 = Blob::<29>::try_from(user.as_slice()).expect("error transformation");
                STAKERS.with(|v| {
                    v.borrow_mut().insert(
                        blob29,
                        UserDetail {
                            balance: r.balance + increase_balance,
                            ..r
                        },
                    );
                });
                Ok(())
            }
        }
    }

    /// Writes the claimed reward for a user.
    ///
    /// # Arguments
    ///
    /// * `user` - A `Principal` representing the user.
    /// * `claimed_dod` - A `u64` representing the claimed reward to be set.
    ///
    /// # Returns
    ///
    /// * `Result<(), String>` - On success, returns `Ok(())`. On failure, returns an error message as a `String`.
    pub fn write_user_claimed_dod(user: Principal, claimed_dod: u64) -> Result<(), String> {
        match Self::get_user_detail(user) {
            None => Err("No user found".to_string()),
            Some(r) => {
                let blob29 = Blob::<29>::try_from(user.as_slice()).expect("error transformation");
                STAKERS.with(|v| {
                    v.borrow_mut()
                        .insert(blob29, UserDetail { claimed_dod, ..r });
                });
                Ok(())
            }
        }
    }

    /// Writes the claimed reward for a miner.
    ///
    /// # Arguments
    ///
    /// * `user` - A `Principal` representing the miner.
    /// * `claimed_dod` - A `u64` representing the claimed reward to be set.
    ///
    /// # Returns
    ///
    /// * `Result<(), String>` - On success, returns `Ok(())`. On failure, returns an error message as a `String`.
    pub fn write_miner_claimed_dod(user: Principal, claimed_dod: u64) -> Result<(), String> {
        match Self::get_miner_by_principal(user) {
            None => Err("No miner found".to_string()),
            Some(r) => {
                MINERS.with(|v| {
                    v.borrow_mut().insert(
                        BtcAddress(r.btc_address.clone()),
                        MinerInfo { claimed_dod, ..r },
                    );
                });
                Ok(())
            }
        }
    }

    /// Executes cycles on block data by burning the specified amount of cycles.
    ///
    /// # Arguments
    ///
    /// * `to_burn` - A `u128` representing the amount of cycles to burn.
    ///
    /// # Returns
    ///
    /// * `Result<(), String>` - On success, returns `Ok(())`. On failure, returns an error message as a `String`.
    pub fn execute_cycles_on_block_data(to_burn: u128) -> Result<(), String> {
        let current_balance = ic_cdk::api::canister_balance128();
        if current_balance < to_burn {
            ic_cdk::println!(
                "{}",
                format!(
                    "Insufficient cycles, current {:?}, expect to burn {:?}",
                    current_balance, to_burn
                )
            );
            Ok(())
        } else {
            ic_cdk::api::cycles_burn(to_burn.saturating_sub(CYCLES_BURNER_FEE));
            Ok(())
        }
    }

    /// Places an order for a user over a range of blocks.
    ///
    /// This function updates the new user orders and new block orders with the specified range and amount.
    ///
    /// # Arguments
    ///
    /// * `user` - A `Principal` representing the user placing the order.
    /// * `range` - A `BlockRange` representing the range of blocks for the order.
    /// * `amount` - A `u128` representing the amount for the order.
    pub fn user_put_order_v2(user: Principal, range: BlockRange, amount: u128) {
        // Update the new user orders with the specified range and amount.

        let old = NEW_USER_ORDERS.with_borrow(|v| v.get(&user));

        NEW_USER_ORDERS.with_borrow_mut(|v| {
            NewUserOrders::update_order(v, user, range, amount);
        });

        // Update the new block orders for each block in the specified range.
        NEW_BLOCK_ORDERS.with_borrow_mut(|v| {
            for block in range.0..range.1 {
                NewBlockOrders::write_order_by_block_height(
                    v,
                    block,
                    user,
                    amount,
                    OrderStatus::Pending,
                );
            }

            if old.is_some() {
                let _old = old.unwrap();
                if _old.r.1 > range.1 {
                    for block in range.1.._old.r.1 {
                        NewBlockOrders::write_order_by_block_height(
                            v,
                            block,
                            user,
                            0,
                            OrderStatus::Cancelled,
                        );
                    }
                }
            }
        });
    }

    /// Updates the balances of users based on block orders.
    ///
    /// This function iterates through the block orders and updates the balance of each user.
    /// If the user's balance is greater than the order amount and the user has a bet in the range,
    /// it subtracts the order amount from the balance. Otherwise, the balance remains unchanged.
    /// It also calculates the user's share of the total cycles and updates their total DOD reward.
    ///
    /// # Arguments
    ///
    /// * `block` - A `Height` representing the block height.
    /// * `total_cycles` - A `u128` representing the total cycles for the block.
    pub fn update_users_balance_v2(block: Height, total_cycles: u128) {
        NEW_BLOCK_ORDERS.with_borrow_mut(|s| {
            let orders: Vec<_> = NewBlockOrders::get_orders_by_block_height(s, block).collect();
            for (p, v) in orders {
                match Self::get_user_detail(p) {
                    None => {
                        continue;
                    }
                    Some(user) => {
                        // Check if the user has a bet in the range.
                        let is_range = NewUserOrders::get_user_bet(user.principal, block).is_some();
                        let OrderDetail {
                            value: user_bet,
                            status,
                        } = v;
                        // Calculate the new balance.
                        let mut actual_bet = user_bet;
                        let new_balance = if user.balance >= user_bet
                            && is_range
                            && status != OrderStatus::Cancelled
                            && status != OrderStatus::Filled
                        {
                            user.balance - user_bet
                        } else {
                            actual_bet = 0;
                            user.balance
                        };
                        let blob29 =
                            Blob::<29>::try_from(p.as_slice()).expect("error transformation");

                        // Calculate the user's share and reward.

                        let share = actual_bet as f64 / total_cycles as f64;
                        let halving_settings =
                            Self::get_halving_settings().expect("Can not get halving settings");
                        let reward =
                            Self::get_block_reward_by_height(block, Some(halving_settings))
                                .expect("Can not get block reward by height");
                        let r = (reward as f64 * share).floor() as u64;

                        if status == OrderStatus::Pending {
                            NewBlockOrders::write_order_by_block_height(
                                s,
                                block,
                                p,
                                user_bet,
                                OrderStatus::Filled,
                            );
                        }

                        // Update the user's details in the STAKERS map.
                        STAKERS.with(|v| {
                            v.borrow_mut().insert(
                                blob29,
                                UserDetail {
                                    balance: new_balance,
                                    total_dod: user.total_dod + r,
                                    ..user
                                },
                            );
                        });
                    }
                }
            }
        })
    }

    /// Retrieves the range of blocks for a given user.
    ///
    /// This function fetches the range of blocks that a user has set orders for.
    ///
    /// # Arguments
    ///
    /// * `user` - A `Principal` representing the user whose block range is to be retrieved.
    ///
    /// # Returns
    ///
    /// * `Option<NewBlockOrderValue>` - Returns `Some(NewBlockOrderValue)` if the user has set a range, otherwise `None`.
    pub fn get_user_range(user: Principal) -> Option<NewBlockOrderValue> {
        NewUserOrders::get_user_set_range(user)
    }

    /// Retrieves the share of a user in a specific block.
    ///
    /// This function calculates the share of a user in a specific block based on the total cycles and the user's block order.
    ///
    /// # Arguments
    ///
    /// * `block` - A `u64` representing the block height.
    /// * `user` - A `Principal` representing the user.
    ///
    /// # Returns
    ///
    /// * `f64` - The share of the user in the block.
    pub fn get_user_block_share(block: u64, user: Principal) -> f64 {
        let total_cycles = Self::get_block_total_cycles(block, false);
        let user_order = Self::get_user_block_order(user, block);

        if user.ne(&id())
            && (user_order.status == OrderStatus::Pending
                || user_order.status == OrderStatus::Cancelled)
        {
            0f64
        } else {
            user_order.value as f64 / total_cycles as f64
        }
    }

    /// Retrieves the user's block reward and share.
    ///
    /// This function calculates the user's reward and share for a specific block based on the total cycles and the user's block order.
    ///
    /// # Arguments
    ///
    /// * `block` - A `u64` representing the block height.
    /// * `user` - A `Principal` representing the user.
    ///
    /// # Returns
    ///
    /// * `(u64, f64)` - A tuple containing the user's reward as `u64` and the share as `f64`.
    pub fn get_user_block_reward(block: u64, user: Principal) -> (u64, f64) {
        let share = Self::get_user_block_share(block, user);
        let halving_settings = Self::get_halving_settings().expect("Can not get halving settings");
        let reward = Self::get_block_reward_by_height(block, Some(halving_settings))
            .expect("Can not get block reward by height");
        ((reward as f64 * share).floor() as u64, share)
    }

    /// Retrieves the total cycles for a specific block.
    ///
    /// This function calculates the total cycles for a given block by summing up the cycles from all orders.
    ///
    /// # Arguments
    ///
    /// * `block` - A `u64` representing the block height.
    ///
    /// # Returns
    ///
    /// * `u128` - The total cycles for the block.
    pub fn get_block_total_cycles(block: u64, with_filled: bool) -> u128 {
        NEW_BLOCK_ORDERS.with_borrow(|v| {
            NewBlockOrders::get_orders_by_block_height(v, block).fold(0, |acc, (_, x)| {
                match (with_filled, x.status) {
                    (true, OrderStatus::Filled) | (_, OrderStatus::Cancelled) => acc,
                    _ => acc + x.value,
                }
            })
        })
    }

    /// Retrieves the block order for a specific user and block.
    ///
    /// This function fetches the order details for a given user and block height.
    /// It first attempts to retrieve the order from the new block orders. If no order is found,
    /// it returns a default `OrderDetail` with a value of 0 and a status of `Pending`.
    ///
    /// # Arguments
    ///
    /// * `user` - A `Principal` representing the user whose order is to be retrieved.
    /// * `block` - A `u64` representing the block height.
    ///
    /// # Returns
    ///
    /// * `OrderDetail` - The order details for the specified user and block.
    pub fn get_user_block_order(user: Principal, block: u64) -> OrderDetail {
        NEW_BLOCK_ORDERS.with_borrow(|v| {
            NewBlockOrders::get_orders_by_block_height(v, block)
                .filter(|(k, _)| k == &user)
                .map(|(_, v)| v)
                .next()
                .unwrap_or(OrderDetail {
                    value: 0,
                    status: OrderStatus::Pending,
                })
        })
    }

    /// Retrieves the user's orders within a specified block range.
    ///
    /// This function fetches the orders for a given user within the specified block range.
    /// It accesses the `NEW_BLOCK_ORDERS` to get the user's orders in the range and collects them into a vector.
    ///
    /// # Arguments
    ///
    /// * `user` - A `Principal` representing the user whose orders are to be retrieved.
    /// * `range` - A `BlockRange` representing the range of blocks to fetch orders from.
    ///
    /// # Returns
    ///
    /// * `Vec<(u64, OrderDetail)>` - A vector of tuples where each tuple contains a block height (`u64`) and the corresponding `OrderDetail`.
    pub fn get_user_orders(user: Principal, range: BlockRange) -> Vec<(u64, OrderDetail)> {
        NEW_BLOCK_ORDERS.with_borrow(|v| {
            NewBlockOrders::get_user_orders_in_range(v, user, range)
                .collect::<Vec<(u64, OrderDetail)>>()
        })
    }

    /// Retrieves the user's orders within a specified block range and filters them by status.
    ///
    /// This function fetches the orders for a given user within the specified block range and filters them by the provided status.
    /// It accesses the `NEW_BLOCK_ORDERS` to get the user's orders in the range, calculates the reward and share for each order,
    /// and collects them into a vector.
    ///
    /// # Arguments
    ///
    /// * `user` - A `Principal` representing the user whose orders are to be retrieved.
    /// * `from` - A `u64` representing the starting block height.
    /// * `to` - A `u64` representing the ending block height.
    /// * `status` - An `OrderStatus` representing the status to filter orders by.
    ///
    /// # Returns
    ///
    /// * `(Vec<UserBlockOrder>, u64)` - A tuple where the first element is a vector of `UserBlockOrder` and the second element is the total number of orders.
    pub fn get_user_orders_by_blocks(
        user: Principal,
        from: u64,
        to: u64,
        status: OrderStatus,
    ) -> (Vec<UserBlockOrder>, u64) {
        NEW_BLOCK_ORDERS.with_borrow(|v| {
            let data = NewBlockOrders::get_user_orders_in_range(v, user, (from, to))
                .filter(|(_, v)| v.status == status)
                .map(|(a, b)| {
                    let (reward, share) = Self::get_user_block_reward(a.clone(), user.clone());
                    UserBlockOrder {
                        block: a.clone(),
                        amount: b.value.clone(),
                        share,
                        reward,
                    }
                })
                .collect::<Vec<UserBlockOrder>>();
            let total = data.len() as u64;
            (data, total)
        })
    }

    /// Retrieves orders by block range.
    ///
    /// This function fetches the orders for a specified block range and collects them into a vector of `BlockDataFull`.
    /// It accesses the `NEW_BLOCK_ORDERS` to get the orders for each block in the range, filters the filled orders,
    /// and collects the user data and miner candidates for each block.
    ///
    /// # Arguments
    ///
    /// * `from` - A `u64` representing the starting block height.
    /// * `to` - A `u64` representing the ending block height.
    ///
    /// # Returns
    ///
    /// * `Vec<BlockDataFull>` - A vector of `BlockDataFull` containing the block data, user data, and miner candidates for each block in the range.
    pub fn get_orders_by_block_v2(from: u64, to: u64) -> Vec<BlockDataFull> {
        let mut data: Vec<BlockDataFull> = vec![];
        NEW_BLOCK_ORDERS.with_borrow(|v| {
            for i in from..to {
                if let Some(block) = BLOCKS.with_borrow(|v| v.get(&i)) {
                    let miners = CANDIDATES.with_borrow(|v| {
                        v.get(&i).map_or_else(Vec::new, |v| {
                            v.candidates
                                .iter()
                                .map(|(_, k)| {
                                    let principal = MINERS.with_borrow(|s| {
                                        let info =
                                            s.get(&BtcAddress(k.btc_address.clone())).unwrap();
                                        info.owner.clone()
                                    });
                                    MinerCandidateExt {
                                        miner_principal: principal,
                                        btc_address: k.btc_address.clone(),
                                        submit_time: k.submit_time.clone(),
                                        cycles_price: k.cycles_price.clone(),
                                        signed_commit_psbt: k.signed_commit_psbt.clone(),
                                        signed_reveal_psbt: k.signed_reveal_psbt.clone(),
                                    }
                                })
                                .collect()
                        })
                    });

                    let orders = NewBlockOrders::get_orders_by_block_height(v, i);
                    let user_data: Vec<UserBlockOrderData> = orders
                        .into_iter()
                        .filter(|(_, v)| v.status == OrderStatus::Filled)
                        .map(|(user, amount)| {
                            let (reward, share) = Self::get_user_block_reward(i, user);
                            UserBlockOrderData {
                                height: i,
                                amount: amount.value,
                                share,
                                reward,
                                user,
                            }
                        })
                        .collect();

                    data.push(BlockDataFull {
                        block,
                        user_data,
                        miners,
                    });
                } else {
                    break;
                }
            }
        });
        data
    }

    /// Claims the reward for a user.
    ///
    /// This asynchronous function calculates the amount of DOD tokens to be claimed by the user,
    /// updates the user's claimed DOD amount, and transfers the tokens to the user's account.
    ///
    /// # Arguments
    ///
    /// * `user` - A `Principal` representing the user claiming the reward.
    ///
    /// # Returns
    ///
    /// * `Result<Nat, String>` - On success, returns the amount of tokens claimed as `Nat`. On failure, returns an error message as a `String`.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// * The user details cannot be retrieved.
    /// * The claimed DOD amount cannot be written.
    /// * The token canister cannot be retrieved.
    /// * The transfer call to the token canister fails.
    pub async fn claim_reward(user: Principal) -> Result<Nat, String> {
        let user_detail = Self::get_user_detail(user).unwrap();
        let to_claim = if user_detail.total_dod > user_detail.claimed_dod {
            user_detail.total_dod - user_detail.claimed_dod
        } else {
            0
        };
        Self::write_user_claimed_dod(user_detail.principal, user_detail.total_dod)?;

        let token_canister = Self::get_token_canister()?;
        let amount = NumTokens::from(to_claim);
        let arg = TransferArg {
            from_subaccount: None,
            to: Account {
                owner: user.clone(),
                subaccount: None,
            },
            fee: None,
            created_at_time: Some(ic_cdk::api::time()),
            memo: Some(icrc_ledger_types::icrc1::transfer::Memo::from(
                MEMO_TRANSFER,
            )),
            amount: amount.clone(),
        };
        let call_result = ic_cdk::api::call::call(token_canister, "icrc1_transfer", (arg.clone(),))
            .await
            as Result<(Result<Nat, TransferError>,), (RejectionCode, String)>;

        match call_result {
            Ok(resp) => match resp.0 {
                Ok(_resp) => Ok(_resp),
                Err(msg) => Err(format!(
                    "Error calling claim_reward::icrc1_transfer msg: {}",
                    msg
                )),
            },
            Err((code, msg)) => {
                let code = code as u16;
                Err(format!(
                    "Error calling claim_reward::icrc1_transfer code: {}, msg: {}",
                    code, msg
                ))
            }
        }
    }

    /// Retrieves the block reward for a given block height, considering halving settings.
    ///
    /// This function calculates the block reward based on the default rewards and the halving ratio
    /// if the halving settings are provided. The reward is adjusted according to the current halving ratio.
    ///
    /// # Arguments
    ///
    /// * `height` - A `Height` representing the block height.
    /// * `halving_settings` - An `Option<HalvingSettings>` representing the halving settings.
    ///
    /// # Returns
    ///
    /// * `Result<u64, String>` - On success, returns the calculated block reward as `u64`. On failure, returns an error message as a `String`.
    pub fn get_block_reward_by_height(
        height: Height,
        halving_settings: Option<HalvingSettings>,
    ) -> Result<u64, String> {
        let default_reward = Self::get_default_rewards()?;
        let mut reward = default_reward;
        if halving_settings.is_some() {
            let ratio = config::get_current_halving_ratio(height, halving_settings.unwrap());
            reward = (reward as f64 * ratio).floor() as u64;
        }
        Ok(reward)
    }

    /// Mints DOD award to the treasury.
    ///
    /// This asynchronous function transfers the specified reward amount to the DOD treasury subaccount.
    /// It constructs a transfer argument and calls the `icrc1_transfer` method on the token canister.
    ///
    /// # Arguments
    ///
    /// * `reward` - A `u64` representing the amount of DOD tokens to be minted.
    ///
    /// # Returns
    ///
    /// * `Result<Nat, String>` - On success, returns the amount of tokens minted as `Nat`. On failure, returns an error message as a `String`.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// * The DOD block account cannot be retrieved.
    /// * The token canister cannot be retrieved.
    /// * The transfer call to the token canister fails.
    pub async fn mint_dod_award_to_treasury(reward: u64) -> Result<Nat, String> {
        let to_subaccount = Self::get_dod_block_account()?;
        let token_canister = Self::get_token_canister()?;
        let amount = NumTokens::from(reward);
        let arg = TransferArg {
            from_subaccount: None,
            to: Account {
                owner: id(),
                subaccount: Some(to_subaccount),
            },
            fee: None,
            created_at_time: Some(ic_cdk::api::time()),
            memo: Some(icrc_ledger_types::icrc1::transfer::Memo::from(
                MEMO_TRANSFER,
            )),
            amount: amount.clone(),
        };

        ic_cdk::println!(
            "mint_dod_award_to_treasury::icrc1_transfer arg:{}",
            format!(" {:?}", arg)
        );

        let call_result = ic_cdk::api::call::call(token_canister, "icrc1_transfer", (arg.clone(),))
            .await
            as Result<(Result<Nat, TransferError>,), (RejectionCode, String)>;

        match call_result {
            Ok(resp) => match resp.0 {
                Ok(_resp) => {
                    ic_cdk::println!(
                        "mint_dod_award_to_treasury::icrc1_transfer resp:{}",
                        format!(" {:?}", _resp)
                    );
                    Ok(_resp)
                }
                Err(msg) => {
                    ic_cdk::println!(
                        "mint_dod_award_to_treasury::icrc1_transfer msg:{}",
                        format!(" {:?}", msg)
                    );
                    Err(format!(
                        "Error calling mint_dod_award::icrc1_transfer msg: {}",
                        msg
                    ))
                }
            },
            Err((code, msg)) => {
                let code = code as u16;
                ic_cdk::println!(
                    "Error calling mint_dod_award_to_treasury::icrc1_transfer msg:{}",
                    format!(" {:?}", msg)
                );
                Err(format!(
                    "Error calling mint_dod_award::icrc1_transfer code: {}, msg: {}",
                    code, msg
                ))
            }
        }
    }

    /// Burns DOD tokens from the treasury.
    ///
    /// This asynchronous function transfers the specified amount of DOD tokens from the treasury subaccount
    /// to the user's account. It constructs a transfer argument and calls the `icrc1_transfer` method on the token canister.
    ///
    /// # Arguments
    ///
    /// * `user` - A `Principal` representing the user to whom the tokens will be transferred.
    /// * `total_burn` - A `u64` representing the amount of DOD tokens to be burned.
    ///
    /// # Returns
    ///
    /// * `Result<Nat, String>` - On success, returns the amount of tokens burned as `Nat`. On failure, returns an error message as a `String`.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// * The token canister cannot be retrieved.
    /// * The DOD block account cannot be retrieved.
    /// * The transfer call to the token canister fails.
    pub async fn burn_dod_from_treasury(user: Principal, total_burn: u64) -> Result<Nat, String> {
        if total_burn > 0 {
            let token_canister = Self::get_token_canister()?;
            let from_subaccount = Self::get_dod_block_account()?;
            let amount = NumTokens::from(total_burn);
            let arg = TransferArg {
                from_subaccount: Some(from_subaccount),
                to: Account {
                    owner: user.clone(),
                    subaccount: None,
                },
                fee: None,
                created_at_time: Some(ic_cdk::api::time()),
                memo: Some(icrc_ledger_types::icrc1::transfer::Memo::from(
                    MEMO_BURN_DOD,
                )),
                amount: amount.clone(),
            };

            ic_cdk::println!(
                "burn_dod_from_treasury::icrc1_transfer arg:{}",
                format!(" {:?}", arg)
            );

            let call_result =
                ic_cdk::api::call::call(token_canister, "icrc1_transfer", (arg.clone(),)).await
                    as Result<(Result<Nat, TransferError>,), (RejectionCode, String)>;

            match call_result {
                Ok(resp) => match resp.0 {
                    Ok(_resp) => {
                        ic_cdk::println!(
                            "burn_dod_from_treasury::icrc1_transfer resp:{}",
                            format!(" {:?}", _resp)
                        );
                        Ok(_resp)
                    }
                    Err(msg) => {
                        ic_cdk::println!(
                            "Error calling  burn_dod_from_treasury::icrc1_transfer msg:{}",
                            format!(" {:?}", msg)
                        );
                        Err(format!(
                            "Error calling burn_dod_from_treasury::icrc1_transfer msg: {}",
                            msg
                        ))
                    }
                },
                Err((code, msg)) => {
                    let code = code as u16;
                    ic_cdk::println!(
                        "Error calling burn_dod_from_treasury::icrc1_transfer msg:{}",
                        format!(" {:?}", msg)
                    );
                    Err(format!(
                        "Error calling burn_dod_from_treasury::icrc1_transfer code: {}, msg: {}",
                        code, msg
                    ))
                }
            }
        } else {
            Ok(Nat::from(0u64))
        }
    }
}
