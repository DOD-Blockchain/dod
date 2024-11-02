// ------------------
//
// **Here are ego dependencies, needed for ego injections**
//
// ------------------
// BTreeMap
use std::collections::BTreeMap;
use std::str::FromStr;
// ego_macros
use ego_macros::{inject_app_info_api, inject_ego_api};

// ic_cdk
use candid::candid_method;
use candid::Principal;

// ------------------
//
// **Project dependencies
//
// ------------------
// injected macros
use dod_mod::service::DodService;
use dod_mod::state::*;
use dod_mod::types::UserDetail;
use dod_utils::types::{
    BlockData, BlockDataFull, BlockSigs, BootStrapParams, DodCanisters, HalvingSettings, Height,
    MinerBlockData, MinerCandidate, MinerInfo, MinerSubmitPayload, MinerSubmitResponse,
    NewBlockOrderValue, OrderStatus, UserBlockOrderRes,
};
use ic_cdk::caller;
use ic_cdk_macros::*;
use ic_ledger_types::Subaccount;
use icrc_ledger_types::icrc1::account::Account;

// ------------------
//
// ** injections
//
// ------------------
// injection ego apis
inject_ego_api!();
inject_app_info_api!();

#[cfg(not(feature = "no_candid"))]
#[init]
#[candid_method(init, rename = "init")]
fn canister_init() {
    let caller = caller();
    info_log_add(format!("dod: init, caller is {}", caller.clone()).as_str());
    owner_add(caller);
}

#[pre_upgrade]
pub fn pre_upgrade() {
    dod_mod::state::pre_upgrade()
}

#[post_upgrade]
pub fn post_upgrade() {
    dod_mod::state::post_upgrade();
}

#[cfg(not(feature = "no_candid"))]
#[update(name = "whoAmI", guard = "owner_guard")]
#[candid_method(update, rename = "whoAmI")]
pub fn who_am_i() -> Principal {
    ic_cdk::api::caller()
}

#[cfg(not(feature = "no_candid"))]
#[update(name = "bootstrap", guard = "owner_guard")]
#[candid_method(update, rename = "bootstrap")]
pub fn bootstrap(params: BootStrapParams) {
    DodService::new(
        params.block_timer,
        params.difficulty_epoch,
        params.default_rewards,
        params.halving_settings,
        params.dod_block_sub_account,
        params.dod_token_canister,
        params.start_difficulty,
    );
}

#[cfg(not(feature = "no_candid"))]
#[update(name = "add_archive_wasm", guard = "owner_guard")]
#[candid_method(update, rename = "add_archive_wasm")]
pub fn add_archive_wasm(wasm: Vec<u8>) -> Result<(), String> {
    DodService::get_current_service()
        .and_then(|mut service| {
            service.add_archive_wasm(wasm);
            Some(())
        })
        .ok_or_else(|| "No service found".to_string())
}

#[cfg(not(feature = "no_candid"))]
#[update(name = "add_index_wasm", guard = "owner_guard")]
#[candid_method(update, rename = "add_index_wasm")]
pub fn add_index_wasm(wasm: Vec<u8>) -> Result<(), String> {
    DodService::get_current_service()
        .and_then(|mut service| {
            service.add_index_wasm(wasm);
            Some(())
        })
        .ok_or_else(|| "No service found".to_string())
}

#[cfg(not(feature = "no_candid"))]
#[update(name = "add_ledger_wasm", guard = "owner_guard")]
#[candid_method(update, rename = "add_ledger_wasm")]
pub fn add_ledger_wasm(wasm: Vec<u8>) -> Result<(), String> {
    DodService::get_current_service()
        .and_then(|mut service| {
            service.add_ledger_wasm(wasm);
            Some(())
        })
        .ok_or_else(|| "No service found".to_string())
}

#[cfg(not(feature = "no_candid"))]
#[update(name = "set_dod_canisters", guard = "owner_guard")]
#[candid_method(update, rename = "set_dod_canisters")]
pub fn set_dod_canisters(canisters: DodCanisters) {
    DodService::set_token_canister(canisters.ledger);
    DodService::set_dod_canisters(canisters);
}

#[cfg(not(feature = "no_candid"))]
#[query(name = "get_dod_canister")]
#[candid_method(query, rename = "get_dod_canister")]
pub fn get_dod_canister() -> Result<Principal, String> {
    DodService::get_token_canister()
}

#[cfg(not(feature = "no_candid"))]
#[query(name = "get_ledger_wasm", guard = "owner_guard")]
#[candid_method(query, rename = "get_ledger_wasm")]
pub fn get_ledger_wasm() -> Option<Vec<u8>> {
    DodService::get_current_service().and_then(|service| service.ledger_wasm)
}

#[cfg(not(feature = "no_candid"))]
#[update(name = "deploy_canisters", guard = "owner_guard")]
#[candid_method(update, rename = "deploy_canisters")]
pub async fn deploy_canisters() -> Result<Principal, String> {
    if let Some(service) = DodService::get_current_service() {
        service.deploy_dod_ledger().await
    } else {
        Err("No service found".to_string())
    }
}

#[cfg(not(feature = "no_candid"))]
#[update(name = "reset_ledgers", guard = "owner_guard")]
#[candid_method(update, rename = "reset_ledgers")]
pub async fn reset_ledgers() -> Result<(), String> {
    if let Some(service) = DodService::get_current_service() {
        service.reset_ledgers().await
    } else {
        Err("No service found".to_string())
    }
}

#[cfg(not(feature = "no_candid"))]
#[update(name = "upgrade_ledger", guard = "owner_guard")]
#[candid_method(update, rename = "upgrade_ledger")]
pub async fn upgrade_ledger() -> Result<(), String> {
    if let Some(service) = DodService::get_current_service() {
        service.upgrade_ledger().await
    } else {
        Err("No service found".to_string())
    }
}

#[cfg(not(feature = "no_candid"))]
#[query(name = "get_deployed_canisters", guard = "owner_guard")]
#[candid_method(query, rename = "get_deployed_canisters")]
pub async fn get_deployed_canisters() -> Option<DodCanisters> {
    DodService::get_dod_canisters()
}

#[cfg(not(feature = "no_candid"))]
#[update(name = "set_halving_settings", guard = "owner_guard")]
#[candid_method(update, rename = "set_halving_settings")]
pub fn set_halving_settings(settings: HalvingSettings) -> Result<(), String> {
    DodService::set_halving_settings(settings)
}

#[cfg(not(feature = "no_candid"))]
#[query(name = "get_halving_settings", guard = "owner_guard")]
#[candid_method(query, rename = "get_halving_settings")]
pub fn get_halving_settings() -> Option<HalvingSettings> {
    DodService::get_halving_settings()
}

#[cfg(not(feature = "no_candid"))]
#[update(name = "register", guard = "anon_guard")]
#[candid_method(update, rename = "register")]
pub fn register(address: String, ecdsa_pubkey: String) -> Result<MinerInfo, String> {
    let pubkey = hex::decode(ecdsa_pubkey).map_err(|_| "Can not decode ecdsa pubkey")?;
    let miner = DodService::register_miner(caller(), address, pubkey)?;
    DodService::register_user(caller())
        .map(|_| miner)
        .map_err(|e| e)
}

#[cfg(not(feature = "no_candid"))]
#[update(name = "start_generating_blocks", guard = "owner_guard")]
#[candid_method(update, rename = "start_generating_blocks")]
pub async fn start_generating_blocks() -> Result<(), String> {
    DodService::start_generate_blocks().await
}

#[cfg(not(feature = "no_candid"))]
#[update(name = "clean_up", guard = "owner_guard")]
#[candid_method(update, rename = "clean_up")]
pub fn clean_up() {
    DodService::clean_up()
}

#[cfg(not(feature = "no_candid"))]
#[query(name = "get_last_block")]
#[candid_method(query, rename = "get_last_block")]
pub fn get_last_block() -> Option<(u64, BlockData)> {
    DodService::get_last_block()
}

#[cfg(not(feature = "no_candid"))]
#[query(name = "get_blocks_range")]
#[candid_method(query, rename = "get_blocks_range")]
pub fn get_blocks_range(from: Height, to: Height) -> Vec<BlockData> {
    DodService::get_blocks_range(from, to)
}

#[cfg(not(feature = "no_candid"))]
#[update(name = "miner_submit_hash")]
#[candid_method(update, rename = "miner_submit_hash")]
pub fn miner_submit_hash(payload: MinerSubmitPayload) -> Result<MinerSubmitResponse, String> {
    let caller = caller();
    DodService::miner_submit_hashes(
        caller,
        payload.btc_address,
        payload.signed_commit_psbt,
        payload.signed_reveal_psbt,
        payload.cycles_price,
    )
}

#[cfg(not(feature = "no_candid"))]
#[query(name = "load_sigs_by_height")]
#[candid_method(query, rename = "load_sigs_by_height")]
pub fn load_sigs_by_height(height: Height) -> Option<BlockSigs> {
    DodService::load_sigs_by_height(height)
}

#[cfg(not(feature = "no_candid"))]
#[query(name = "get_history_miner_candidates")]
#[candid_method(query, rename = "get_history_miner_candidates")]
pub fn get_history_miner_candidates(height: Height) -> Result<Vec<MinerCandidate>, String> {
    let last_block_height = DodService::get_last_block()
        .ok_or_else(|| "Can not get last block".to_string())?
        .0;

    if height >= last_block_height {
        Err("Only before last block data is available".to_string())
    } else {
        Ok(DodService::get_block_candidates(height))
    }
}

#[cfg(not(feature = "no_candid"))]
#[query(name = "get_mining_history_for_miners", guard = "anon_guard")]
#[candid_method(query, rename = "get_mining_history_for_miners")]
pub fn get_mining_history_for_miners(
    btc_address: String,
    from: Height,
    to: Height,
) -> Vec<MinerBlockData> {
    DodService::get_mining_history_for_miners(btc_address, (from, to))
}

#[cfg(not(feature = "no_candid"))]
#[update(name = "user_register", guard = "anon_guard")]
#[candid_method(update, rename = "user_register")]
pub fn user_register() -> Result<(), String> {
    DodService::register_user(caller())
}

#[cfg(not(feature = "no_candid"))]
#[update(name = "deposit_cycles_from_icp", guard = "anon_guard")]
#[candid_method(update, rename = "deposit_cycles_from_icp")]
pub async fn deposit_cycles_from_icp(amount: u64) -> Result<(), String> {
    DodService::deposit_cycles_from_icp(caller(), amount).await;
    Ok(())
}

#[cfg(not(feature = "no_candid"))]
#[update(name = "user_set_burning_rate_combine", guard = "anon_guard")]
#[candid_method(update, rename = "user_set_burning_rate_combine")]
pub fn user_set_burning_rate_combine(br: u128, height: Height, amount: u128) -> Result<(), String> {
    let caller = caller();
    DodService::user_set_burnrate(caller, br)?;
    DodService::user_put_burnrate_orders(caller, height, amount)
}

// pub fn user_instant_bid(br: u128, height: Height, amount: u128) -> Result<(), String> {
//     let caller = caller();
//     DodService::user_put_burnrate_orders(caller, height, amount)
// }

#[cfg(not(feature = "no_candid"))]
#[update(name = "user_set_burning_rate", guard = "anon_guard")]
#[candid_method(update, rename = "user_set_burning_rate")]
pub fn user_set_burning_rate(br: u128) -> Result<(), String> {
    DodService::user_set_burnrate(caller(), br)
}

#[cfg(not(feature = "no_candid"))]
#[update(name = "set_difficulty_adjust_epoch", guard = "owner_guard")]
#[candid_method(update, rename = "set_difficulty_adjust_epoch")]
pub fn set_difficulty_adjust_epoch(difficulty_adjust_epoch: u64) -> Result<(), String> {
    DodService::set_difficulty_adjust_epoch(difficulty_adjust_epoch)
}

#[cfg(not(feature = "no_candid"))]
#[query(name = "get_user_orders_by_blocks", guard = "anon_guard")]
#[candid_method(query, rename = "get_user_orders_by_blocks")]
pub fn get_user_orders_by_blocks(from: Height, to: Height) -> UserBlockOrderRes {
    let (data, total) =
        DodService::get_user_orders_by_blocks(caller(), from, to, OrderStatus::Filled);
    UserBlockOrderRes {
        total,
        from,
        to,
        data,
    }
}

#[cfg(not(feature = "no_candid"))]
#[update(name = "inner_transfer_cycles", guard = "anon_guard")]
#[candid_method(update, rename = "inner_transfer_cycles")]
pub fn inner_transfer_cycles(to: Vec<(Principal, u128)>) -> Result<(), String> {
    DodService::inner_transfer_cycles(caller(), to)
}

#[cfg(not(feature = "no_candid"))]
#[query(name = "get_next_difficulty_adjust_height")]
#[candid_method(query, rename = "get_next_difficulty_adjust_height")]
pub fn get_next_difficulty_adjust_height() -> Result<Option<u64>, String> {
    DodService::get_consider_increase()
}

#[cfg(not(feature = "no_candid"))]
#[query(name = "get_user_burning_range", guard = "anon_guard")]
#[candid_method(query, rename = "get_user_burning_range")]
pub fn get_user_burning_range() -> Option<NewBlockOrderValue> {
    DodService::get_user_range(caller())
}

#[cfg(not(feature = "no_candid"))]
#[update(name = "user_put_orders", guard = "anon_guard")]
#[candid_method(update, rename = "user_put_orders")]
pub fn user_put_orders(height: Height, amount: u128) -> Result<(), String> {
    DodService::user_put_burnrate_orders(caller(), height, amount)
}

#[cfg(not(feature = "no_candid"))]
#[query(name = "get_user_detail", guard = "anon_guard")]
#[candid_method(query, rename = "get_user_detail")]
pub fn get_user_detail() -> Option<UserDetail> {
    DodService::get_user_detail(caller())
}

#[cfg(not(feature = "no_candid"))]
#[query(name = "get_user_detail_indexer")]
#[candid_method(query, rename = "get_user_detail_indexer")]
pub fn get_user_detail_indexer(principal: Principal) -> Option<UserDetail> {
    DodService::get_user_detail(principal)
}

#[cfg(not(feature = "no_candid"))]
#[query(name = "get_user_subaccount", guard = "anon_guard")]
#[candid_method(query, rename = "get_user_subaccount")]
pub fn get_user_subaccount(id: Principal) -> Subaccount {
    DodService::user_subaccount(id)
}

#[cfg(not(feature = "no_candid"))]
#[query(name = "get_canister_cycles", guard = "owner_guard")]
#[candid_method(query, rename = "get_canister_cycles")]
pub fn get_canister_cycles() -> u128 {
    ic_cdk::api::canister_balance128()
}

#[cfg(not(feature = "no_candid"))]
#[update(name = "claim_dod_to_wallet", guard = "anon_guard")]
#[candid_method(update, rename = "claim_dod_to_wallet")]
pub async fn claim_dod_to_wallet(
    to: Option<String>,
    claim_amount: Option<u64>,
) -> Result<String, String> {
    let mut _to = None;
    if to.is_some() {
        _to = Some(Account::from_str(to.unwrap().as_str()).unwrap());
    }

    match DodService::claim_reward(caller(), _to, claim_amount).await {
        Ok(res) => Ok(res.to_string()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(not(feature = "no_candid"))]
#[query(name = "is_miner", guard = "anon_guard")]
#[candid_method(query, rename = "is_miner")]
pub fn is_miner(btc_address: String) -> Option<MinerInfo> {
    DodService::check_miner_if_existed(caller(), btc_address)
}

#[cfg(not(feature = "no_candid"))]
#[query(name = "am_i_candidate", guard = "anon_guard")]
#[candid_method(query, rename = "am_i_candidate")]
pub fn am_i_candidate(height: Height) -> bool {
    DodService::get_miner_by_principal(caller())
        .and_then(|miner| DodService::check_if_in_candidate(miner.btc_address, height))
        .is_some()
}

#[cfg(not(feature = "no_candid"))]
#[query(name = "get_orders_by_block_v2", guard = "owner_guard")]
#[candid_method(query, rename = "get_orders_by_block_v2")]
pub fn get_orders_by_block_v2(from: u64, to: u64) -> Vec<BlockDataFull> {
    DodService::get_orders_by_block_v2(from, to)
}

#[cfg(not(feature = "no_candid"))]
#[query(name = "get_block_total_cycles", guard = "anon_guard")]
#[candid_method(query, rename = "get_block_total_cycles")]
pub fn get_block_total_cycles(height: Height) -> u128 {
    DodService::get_block_total_cycles(height, false)
}

#[cfg(not(feature = "no_candid"))]
#[update(name = "blackhole_ledger", guard = "owner_guard")]
#[candid_method(update, rename = "blackhole_ledger")]
pub async fn blackhole_ledger() -> Result<(), String> {
    if let Some(service) = DodService::get_current_service() {
        service.blockhole_ledger().await
    } else {
        Err("No service found".to_string())
    }
}

#[inline(always)]
pub fn anon_guard() -> Result<(), String> {
    let caller = caller();
    if caller == Principal::anonymous() {
        ic_cdk::api::trap(&format!("{} unauthorized", caller));
    } else {
        Ok(())
    }
}
