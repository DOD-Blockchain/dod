use candid::{CandidType, Principal};
use ic_cdk::api::management_canister::main::{
    create_canister, delete_canister, deposit_cycles, install_code, raw_rand, stop_canister,
    update_settings, CanisterInstallMode, CanisterSettings, CreateCanisterArgument,
    InstallCodeArgument, UpdateSettingsArgument,
};
use ic_cdk::api::management_canister::provisional::CanisterIdRecord;

use ego_types::app::EgoError;

pub type Cycles = u128;

#[derive(CandidType)]
struct DepositCyclesArgs {
    pub canister_id: Principal,
}

async fn code_install(
    canister_id: Principal,
    mode: CanisterInstallMode,
    wasm_module: Vec<u8>,
    arg: Vec<u8>,
) -> Result<(), EgoError> {
    let install_config = InstallCodeArgument {
        mode,
        canister_id,
        wasm_module,
        arg,
    };

    match install_code(install_config).await {
        Ok(_) => Ok(()),
        Err((code, msg)) => {
            let code = code as u16;
            Err(EgoError { code, msg })
        }
    }
}

pub async fn canister_main_create(cycles_to_use: Cycles) -> Result<Principal, EgoError> {
    let in_arg = CreateCanisterArgument {
        settings: Some(CanisterSettings {
            controllers: Some(vec![ic_cdk::id()]),
            compute_allocation: None,
            memory_allocation: None,
            freezing_threshold: None,
            reserved_cycles_limit: None,
            log_visibility: None,
            wasm_memory_limit: None,
        }),
    };

    match create_canister(in_arg, cycles_to_use).await {
        Ok(resp) => {
            let canister_id_record = resp.0;
            Ok(canister_id_record.canister_id)
        }
        Err((code, msg)) => {
            let code = code as u16;
            Err(EgoError { code, msg })
        }
    }
}

pub async fn canister_main_delete(canister_id: Principal) -> Result<(), EgoError> {
    // stop the canister
    let _stop_result = match stop_canister(CanisterIdRecord { canister_id }).await {
        Ok(_) => Ok(()),
        Err((code, msg)) => {
            let code = code as u16;
            Err(EgoError { code, msg })
        }
    }?;

    let _delete_result = match delete_canister(CanisterIdRecord { canister_id }).await {
        Ok(_) => Ok(()),
        Err((code, msg)) => {
            let code = code as u16;
            Err(EgoError { code, msg })
        }
    }?;

    Ok(())
}

pub async fn canister_code_reinstall(
    canister_id: Principal,
    wasm_module: Vec<u8>,
    arg: Option<Vec<u8>>,
) -> Result<(), EgoError> {
    code_install(
        canister_id,
        CanisterInstallMode::Reinstall,
        wasm_module,
        arg.unwrap_or(b"".to_vec()),
    )
    .await
}

pub async fn canister_code_install(
    canister_id: Principal,
    wasm_module: Vec<u8>,
    arg: Option<Vec<u8>>,
) -> Result<(), EgoError> {
    code_install(
        canister_id,
        CanisterInstallMode::Install,
        wasm_module,
        arg.unwrap_or(b"".to_vec()),
    )
    .await
}

pub async fn canister_code_upgrade(
    canister_id: Principal,
    wasm_module: Vec<u8>,
    arg: Option<Vec<u8>>,
) -> Result<(), EgoError> {
    code_install(
        canister_id,
        CanisterInstallMode::Upgrade(None),
        wasm_module,
        arg.unwrap_or(b"".to_vec()),
    )
    .await
}

pub async fn canister_cycle_top_up(
    canister_id: Principal,
    cycles_to_use: Cycles,
) -> Result<(), EgoError> {
    match deposit_cycles(CanisterIdRecord { canister_id }, cycles_to_use).await {
        Ok(_) => Ok(()),
        Err((code, msg)) => {
            let code = code as u16;
            Err(EgoError { code, msg })
        }
    }
}

pub async fn canister_add_controllers(
    canister_id: Principal,
    controllers: Vec<Principal>,
) -> Result<(), EgoError> {
    let setting = UpdateSettingsArgument {
        canister_id,
        settings: CanisterSettings {
            controllers: Some(controllers),
            compute_allocation: None,
            memory_allocation: None,
            freezing_threshold: None,
            reserved_cycles_limit: None,
            log_visibility: None,
            wasm_memory_limit: None,
        },
    };

    match update_settings(setting).await {
        Ok(_) => Ok(()),
        Err((code, msg)) => {
            let code = code as u16;
            Err(EgoError { code, msg })
        }
    }
}

pub async fn random_32() -> Result<Vec<u8>, EgoError> {
    match raw_rand().await {
        Ok((v,)) => Ok(v),
        Err((code, msg)) => {
            let code = code as u16;
            Err(EgoError { code, msg })
        }
    }
}
