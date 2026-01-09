// ------------------------------------------------------------
// Copyright (c) Microsoft Corporation.  All rights reserved.
// Licensed under the MIT License (MIT). See License in the repo root for
// license information.
// ------------------------------------------------------------

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // generate kvstore for grpc
    tonic_prost_build::compile_protos("proto/rcstore.proto")?;

    Ok(())
}
