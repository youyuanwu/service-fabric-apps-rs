//! Self-contained native writes; HTTP observes and cleans only the pre-owned name.
use std::{path::Path, time::Duration};

use mssf_client::{Error, FabricClient};
use serde_json::{json, Value};

#[path = "support/name_oracle.rs"]
mod oracle;
use oracle::*;
#[path = "support/fixture.rs"]
mod fixture;

async fn mutate(
    client: &FabricClient,
    journal: &mut Journal,
    path: &Path,
    delete: bool,
    expected_error: Option<i64>,
) -> Result<Option<i64>> {
    journal.record(path, Stage::Pending)?;
    let outcome = if delete {
        client
            .delete_name_with_timeout(&journal.name, Duration::from_secs(30))
            .await
    } else {
        client.create_name(&journal.name).await
    };
    // Unknown errors deliberately keep Pending, including timeout/cancel/disconnect.
    // Even an observed absence cannot prove an abandoned create will not commit later.
    match outcome {
        Ok(()) => {
            journal.record(path, Stage::Settled)?;
            if expected_error.is_some() {
                return Err(
                    "native write unexpectedly succeeded instead of returning an error".into(),
                );
            }
            Ok(None)
        }
        Err(Error::Remote { code, .. }) if code == DUPLICATE || code == MISSING => {
            journal.record(path, Stage::Settled)?;
            if Some(code) != expected_error {
                return Err(format!("unexpected native name error: {code}").into());
            }
            Ok(Some(code))
        }
        Err(error) => Err(error.into()),
    }
}

async fn lifecycle() -> Result<Value> {
    let path = output("MSSF_NAME_CONFIG")?;
    let report = output("MSSF_NAME_REPORT")?;
    if path == report {
        return Err("name journal and report must differ".into());
    }
    match std::fs::remove_file(&report) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let http = Http::new()?;
    let runtime = http.runtime().await?;
    let mut journal = Journal::new(owner()?, std::env::var("MSSF_TEST_ARCHIVE_SHA256")?)?;
    journal.reserve(&path)?;
    confirm_absent(&mut journal, &path, &http).await?;
    let client =
        FabricClient::connect_clear("127.0.0.1:19000".parse()?, Duration::from_secs(30)).await?;
    assert!(!client.name_exists(&journal.name).await?);
    mutate(&client, &mut journal, &path, false, None).await?;
    assert!(http.exists(&journal.name).await?);
    assert!(client.name_exists(&journal.name).await?);
    let duplicate = mutate(&client, &mut journal, &path, false, Some(DUPLICATE)).await?;
    assert!(!client.is_closed());
    assert!(client.name_exists(&journal.name).await?);
    mutate(&client, &mut journal, &path, true, None).await?;
    assert!(!http.exists(&journal.name).await?);
    assert!(!client.name_exists(&journal.name).await?);
    let missing = mutate(&client, &mut journal, &path, true, Some(MISSING)).await?;
    assert!(!client.is_closed());
    assert!(!client.name_exists(&journal.name).await?);
    client.ping().await?;
    client.close().await?;
    Ok(json!({
        "profile": "clear",
        "runtime_version": runtime,
        "archive_sha256": journal.archive_sha256,
        "owned_name": journal.name,
        "initial_http_absent": true,
        "native_create_succeeded": true,
        "http_present_after_create": true,
        "duplicate_create_code": duplicate,
        "reuse_after_duplicate": true,
        "native_delete_succeeded": true,
        "http_absent_after_delete": true,
        "missing_delete_code": missing,
        "reuse_after_missing": true,
        "shared_native_client": true,
    }))
}

#[tokio::test]
#[ignore = "owns a unique name on a disposable onebox; requires MSSF_NAME_* and archive hash"]
async fn onebox_native_name_lifecycle() -> Result<()> {
    let mut report = fixture::with_cleanup(lifecycle(), async {
        cleanup(&output("MSSF_NAME_CONFIG")?, &owner()?, &Http::new()?).await
    })
    .await?;
    // Never publish stale/partial success before owned cleanup has completed.
    report["owned_cleanup_confirmed"] = json!(true);
    std::fs::write(
        output("MSSF_NAME_REPORT")?,
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(())
}
