// ------------------------------------------------------------
// Copyright (c) Microsoft Corporation.  All rights reserved.
// Licensed under the MIT License (MIT). See License in the repo root for
// license information.
// ------------------------------------------------------------

#![allow(
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    clippy::derivable_impls,
    clippy::missing_safety_doc,
    clippy::too_many_arguments,
    clippy::extra_unused_lifetimes,
    clippy::useless_transmute,
    clippy::unused_unit
)]
mod FabricCommon;
pub mod ReliableCollectionRuntime;

// Special usage for mssf_pal.
// See mssf_pal documentations for why this is used this way.
use extern_windows_core::*;
extern crate self as windows;
extern crate self as windows_core;

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PSTR(pub *mut u8);

impl AsRef<PSTR> for PSTR {
    fn as_ref(&self) -> &Self {
        self
    }
}

impl windows_core::TypeKind for PSTR {
    type TypeKind = windows_core::CopyType;
}
