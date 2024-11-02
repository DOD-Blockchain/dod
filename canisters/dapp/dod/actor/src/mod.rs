pub mod actor;

#[allow(unused_imports)]
use candid::Principal;
#[allow(unused_imports)]
use dod_mod::types::*;
#[allow(unused_imports)]
use dod_utils::types::*;
#[allow(unused_imports)]
use ego_types::app::{AppId, Version};
#[allow(unused_imports)]
use ego_types::app_info::AppInfo;
#[allow(unused_imports)]
use ic_ledger_types::Subaccount;
#[allow(unused_imports)]
use icrc_ledger_types::icrc1::account::Account;
#[allow(unused_imports)]
use std::collections::BTreeMap;

candid::export_service!();

#[no_mangle]
pub fn get_candid_pointer() -> *mut std::os::raw::c_char {
    let c_string = std::ffi::CString::new(__export_service()).unwrap();

    c_string.into_raw()
}
