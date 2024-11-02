pub mod block;
pub mod config;
pub mod miner;
pub mod staker;

use crate::common::{
    CMCClient, NotifyTopUpRequest, CMC_CAN_ID, CYCLES_BURNER_FEE, CYCLES_CREATE_FEE, ICP_CAN_ID,
    ICP_FEE, MEMO_BURN_DOD, MEMO_TOP_UP_CANISTER, MEMO_TRANSFER, MIN_ICP_STAKE_E8S_U64,
};
use crate::management::{
    canister_add_controllers, canister_code_install, canister_code_reinstall,
    canister_code_upgrade, canister_main_create, Cycles,
};
use crate::memory::{
    BLOCKS, CANDIDATES, CONFIG, MINERS, NEW_BLOCK_ORDERS, NEW_USER_ORDERS, SIGS, STAKERS, TIMER_IDS,
};
use crate::orders::{NewBlockOrders, NewUserOrders};
use crate::state::{info_log_add, owners};
use crate::types::{
    ArchiveOptions, FeatureFlags, IndexArg, IndexInitArgs, InitArgs, LedgerArgument, UpgradeArgs,
    UserDetail,
};
use base64::Engine;
use candid::{encode_args, CandidType, Deserialize, Encode, Nat, Principal};
use dod_utils::bitwork::{
    bitwork_from_height, bitwork_minus_bit_hex, bitwork_plus_bit_hex, Bitwork,
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

const DIFFICULTY_ADJUST_STEP: u8 = 1;
// const MIN_MINER_PRICE: u128 = 10_000_000_000u128; // 0.1T

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

    pub async fn upgrade_ledger(&self) -> Result<(), String> {
        let leger_canister_id = Self::get_dod_canisters().unwrap().ledger;
        let args = UpgradeArgs {
            metadata: Some(vec![(
                "icrc1:logo".to_string(),
                MetadataValue::from("data:image/webp;base64,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"),
            )]),
            token_name: None,
            token_symbol: Some("DOD".to_string()),
            transfer_fee: None,
            change_fee_collector: None,
            max_memo_length: None,
            feature_flags: None,
            maximum_number_of_accounts: None,
            accounts_overflow_trim_quantity: None,
            change_archive_options: None,
        };
        canister_code_upgrade(
            leger_canister_id,
            self.ledger_wasm.clone().unwrap(),
            Encode!(&LedgerArgument::Upgrade(Some(args))).ok(),
        )
        .await
        .map_err(|e| {
            println!("Error installing index canister: {:?}", e.msg);
            e.msg
        })?;

        Ok(())
    }

    pub async fn blockhole_ledger(&self) -> Result<(), String> {
        let DodCanisters {
            ledger,
            index,
            archive,
        } = Self::get_dod_canisters().unwrap();

        let _ledger_exec = canister_add_controllers(ledger, vec![])
            .await
            .map_err(|e| e.msg)?;
        let _index_exec = canister_add_controllers(index, vec![])
            .await
            .map_err(|e| e.msg)?;
        let _archive_exec = canister_add_controllers(archive, vec![])
            .await
            .map_err(|e| e.msg)?;
        Ok(())
    }

    pub fn set_difficulty_adjust_epoch(epoch: u64) -> Result<(), String> {
        config::set_difficulty_adjust_epoch(epoch)
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

    /// Sets the halving settings.
    ///
    /// This function updates the halving settings in the configuration
    /// with the provided `halving_settings`.
    ///
    /// # Arguments
    ///
    /// * `halving_settings` - A `HalvingSettings` instance representing the new halving settings.
    ///
    /// # Returns
    ///
    /// * `Result<(), String>` - Returns `Ok(())` if the settings were successfully updated,
    ///   otherwise returns an error message as a `String`.
    pub fn set_halving_settings(halving_settings: HalvingSettings) -> Result<(), String> {
        config::set_halving_settings(halving_settings)
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
    pub fn check_miner_if_existed(caller: Principal, _btc_address: String) -> Option<MinerInfo> {
        miner::check_miner_if_existed(caller)
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
        // if cycles_price < MIN_MINER_PRICE {
        //     return Err(format!("Cycles price below {:?} cycles", MIN_MINER_PRICE));
        // }
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
                    Self::get_block_reward_by_height(last_block.height, halving_settings.clone())
                        .unwrap();

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
                                let decreased = bitwork_minus_bit_hex(
                                    last_block.difficulty.clone(),
                                    DIFFICULTY_ADJUST_STEP,
                                )
                                .unwrap();

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
                                bitwork = bitwork_plus_bit_hex(
                                    last_block.difficulty.clone(),
                                    DIFFICULTY_ADJUST_STEP,
                                )
                                .unwrap();
                                Self::set_consider_increase(Some(i + difficulty_adjust_epoch))
                                    .expect("Can not set consider increase height");
                            }
                        }
                    }
                }

                let current_time = ic_cdk::api::time();
                let block_data = BlockData {
                    height: last_block.height + 1,
                    rewards: Self::get_block_reward_by_height(
                        last_block.height + 1,
                        halving_settings.clone(),
                    )
                    .unwrap(),
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

    pub fn decrease_user_cycle_balance(
        user: Principal,
        decreace_balance: Nat,
    ) -> Result<(), String> {
        match Self::get_user_detail(user) {
            None => Err("No user found".to_string()),
            Some(r) => {
                let blob29 = Blob::<29>::try_from(user.as_slice()).expect("error transformation");
                if r.clone().balance - decreace_balance.clone() < Nat::from(0u128) {
                    return Err("Not enough balance".to_string());
                }
                STAKERS.with(|v| {
                    v.borrow_mut().insert(
                        blob29,
                        UserDetail {
                            balance: r.clone().balance - decreace_balance.clone(),
                            ..r.clone()
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
                if _old.r.1 >= range.1 {
                    for block in range.1..=_old.r.1 {
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

    pub fn user_put_order_instant(user: Principal, range: BlockRange, amount: u128) {
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
                            Self::get_block_reward_by_height(block, Some(halving_settings.clone()))
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

    pub fn get_user_block_share_v2(block: u64, user: Principal) -> f64 {
        let total_cycles = Self::get_block_total_cycles_v2(block, true);
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

    pub fn get_user_block_reward_v2(block: u64, user: Principal) -> (u64, f64) {
        let share = Self::get_user_block_share_v2(block, user);
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

    pub fn get_block_total_cycles_v2(block: u64, _with_filled: bool) -> u128 {
        // NEW_BLOCK_ORDERS.with_borrow(|v| {
        //     NewBlockOrders::get_orders_by_block_height(v, block).fold(0, |acc, (_, x)| {
        //         match x.status {
        //             OrderStatus::Pending => acc + x.value,
        //             OrderStatus::Filled => acc,
        //             OrderStatus::Cancelled => acc + x.value,
        //         }
        //     })
        // })

        BLOCKS.with_borrow(|v| {
            v.get(&block).map_or(0, |x| {
                x.cycle_burned * 2 + x.winner.as_ref().map_or(0, |x| x.reward_cycles.unwrap())
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
                        .filter(|(k, v)| {
                            if k.clone() == id() {
                                return true;
                            } else {
                                v.status == OrderStatus::Filled
                            }
                        })
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
    pub async fn claim_reward(
        user: Principal,
        to: Option<Account>,
        claim_amount: Option<u64>,
    ) -> Result<Nat, String> {
        ic_cdk::println!("\n claim_amount {:?}", claim_amount);
        ic_cdk::println!("\n to {:?}", to);
        let user_detail = Self::get_user_detail(user).unwrap();
        let from_subaccount = Self::get_dod_block_account()?;
        let unclaimed = if user_detail.total_dod > user_detail.claimed_dod {
            user_detail.total_dod - user_detail.claimed_dod
        } else {
            0
        };

        if claim_amount.is_none() {
            return Err("Claim amount is none".to_string());
        }

        if claim_amount.is_some() {
            if claim_amount.unwrap() > unclaimed {
                return Err("Claim amount is greater than unclaimed amount ".to_string());
            }
            if claim_amount.unwrap() == 0 {
                return Err("Claim amount is zero ".to_string());
            }
        }

        Self::write_user_claimed_dod(
            user_detail.principal,
            user_detail.claimed_dod + claim_amount.unwrap_or(0),
        )?;

        let token_canister = Self::get_token_canister()?;

        let amount = NumTokens::from(claim_amount.unwrap_or(0));
        let arg = TransferArg {
            from_subaccount: Some(from_subaccount),
            to: to.unwrap_or(Account {
                owner: user.clone(),
                subaccount: None,
            }),
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

    pub fn inner_transfer_cycles(
        caller: Principal,
        to: Vec<(Principal, u128)>,
    ) -> Result<(), String> {
        let range = Self::get_user_range(caller);
        let last_block = Self::get_last_block();
        if last_block.is_none() {
            return Err("No last block found".to_string());
        }

        if range.is_some() && range.unwrap().r.1 > last_block.unwrap().0 {
            Err("Can not transfer cycles when user has orders running".to_string())
        } else {
            let mut total_amount = 0;
            for (_, amount) in to.clone() {
                total_amount += amount;
            }
            let user = Self::get_user_detail(caller).unwrap();
            if user.clone().balance - total_amount.clone() < Nat::from(0u128) {
                Err("Not enough balance".to_string())
            } else {
                let mut total_amount_actual = 0u128;
                for (to, amount) in to {
                    let s = Self::increase_user_cycle_balance(to, Nat::from(amount));
                    if s.is_ok() {
                        total_amount_actual += amount;
                    }
                }
                Self::decrease_user_cycle_balance(caller, Nat::from(total_amount_actual))
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

#[cfg(test)]
mod test {
    use dod_utils::types::HalvingSettings;

    #[test]
    pub fn test_halving() {
        let d = crate::service::DodService::get_block_reward_by_height(
            20000,
            HalvingSettings {
                interval: 20000,
                ratio: 0.5,
            }
            .into(),
        );
        println!("{:?}", d);
    }
}
