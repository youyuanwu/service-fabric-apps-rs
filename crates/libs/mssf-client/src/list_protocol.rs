//! Source-derived list aggregate envelopes and positional record layouts.

use std::{borrow::Cow, collections::BTreeMap, num::NonZeroU64};

use mssf_transport::{
    message::{Message, ACTION, ACTOR},
    serialization::{Decoder, MAX_COLLECTION_COUNT},
};

use crate::{applications::*, services::*, Error, HealthState, Result};

fn paging<'a>(
    args: &mut Vec<(&'static str, Cow<'a, str>)>,
    token: &'a Option<String>,
    max: Option<NonZeroU64>,
) -> Result<()> {
    if let Some(token) = token.as_deref().filter(|s| !s.is_empty()) {
        args.push(("ContinuationToken", Cow::Borrowed(token)));
    }
    if let Some(max) = max {
        if max.get() > i64::MAX as u64 {
            return Err(Error::InvalidInput(
                "MaxResults exceeds native signed range",
            ));
        }
        args.push(("MaxResults", Cow::Owned(max.to_string())));
    }
    Ok(())
}

fn type_selector<'a>(
    args: &mut Vec<(&'static str, Cow<'a, str>)>,
    key: &'static str,
    value: &'a str,
) -> Result<()> {
    if value.is_empty() {
        return Err(Error::InvalidInput("selected type name must not be empty"));
    }
    args.push((key, Cow::Borrowed(value)));
    Ok(())
}

pub(crate) fn encode_application(query: &ApplicationQueryDescription) -> Result<Vec<u8>> {
    let mut args = Vec::new();
    match &query.filter {
        ApplicationListFilter::All => {}
        ApplicationListFilter::Name(name) => {
            crate::protocol::validate_name(name)?;
            args.push(("ApplicationName", Cow::Borrowed(name.as_str())));
        }
        ApplicationListFilter::TypeName(name) => {
            type_selector(&mut args, "ApplicationTypeName", name)?
        }
        ApplicationListFilter::DefinitionKind(filter) => {
            if filter.bits() != 0 {
                args.push((
                    "ApplicationDefinitionKindFilter",
                    Cow::Owned(filter.bits().to_string()),
                ));
            }
        }
    }
    if query.exclude_application_parameters {
        args.push(("ExcludeApplicationParameters", Cow::Borrowed("true")));
    }
    paging(&mut args, &query.continuation_token, query.max_results)?;
    crate::query_protocol::encode("GetApplicationList", &args)
}

pub(crate) fn encode_service(query: &ServiceQueryDescription) -> Result<Vec<u8>> {
    crate::protocol::validate_name(&query.application_name)?;
    if query.application_name == "fabric:/System" {
        return Err(Error::InvalidInput(
            "System service-list profile is unsupported",
        ));
    }
    let mut args = vec![(
        "ApplicationName",
        Cow::Borrowed(query.application_name.as_str()),
    )];
    match &query.filter {
        ServiceListFilter::All => {}
        ServiceListFilter::Name(name) => {
            crate::protocol::validate_name(name)?;
            args.push(("ServiceName", Cow::Borrowed(name.as_str())));
        }
        ServiceListFilter::TypeName(name) => type_selector(&mut args, "ServiceTypeName", name)?,
    }
    paging(&mut args, &query.continuation_token, query.max_results)?;
    crate::query_protocol::encode("GetApplicationServiceList", &args)
}

fn decode_list<T>(
    message: &Message,
    wrapper: i32,
    record: fn(&mut Decoder<'_>) -> Result<T>,
) -> Result<(Vec<T>, Option<String>)> {
    crate::protocol::check_failure(message)?;
    let properties = message.properties()?;
    // Both native aggregates construct bare Message(QueryResult). Validate
    // actor/action if present; absence is not a fabricated default header.
    if message.headers.iter().any(|h| h.id == ACTOR) && properties.actor != 5 {
        return Err(Error::Protocol("list reply is not from NamingGateway"));
    }
    if message.headers.iter().any(|h| h.id == ACTION) && properties.action != "QueryReply" {
        return Err(Error::Protocol("unexpected list reply action"));
    }
    let result = crate::query_protocol::decode_with_paging_policy(
        &message.body,
        |decoder, tag| {
            if tag != wrapper.to_le_bytes() {
                return Err(Error::Protocol("unexpected native list wrapper type"));
            }
            let rows = decoder.object_array(record)?;
            if decoder.pointer(|_, _| Ok(()))?.is_some() {
                return Err(Error::Protocol("list has a single-item payload"));
            }
            Ok(rows)
        },
        true,
    )?;
    if result.kind != 2 {
        return Err(Error::Protocol("query result is not a List"));
    }
    Ok((
        result
            .payload
            .ok_or(Error::Protocol("list has no typed wrapper"))?,
        result.paging.filter(|s| !s.is_empty()),
    ))
}

pub(crate) fn decode_applications(message: &Message) -> Result<ApplicationListPage> {
    let (applications, continuation_token) = decode_list(message, 4, application)?;
    Ok(ApplicationListPage {
        applications,
        continuation_token,
    })
}

pub(crate) fn decode_services(message: &Message) -> Result<ServiceListPage> {
    let (services, continuation_token) = decode_list(message, 2, service)?;
    Ok(ServiceListPage {
        services,
        continuation_token,
    })
}

fn health(decoder: &mut Decoder<'_>) -> Result<HealthState> {
    match HealthState::from_raw(decoder.i64()?) {
        HealthState::Unrecognized(_) => Err(Error::Protocol("unsupported list health state")),
        state => Ok(state),
    }
}

fn name(decoder: &mut Decoder<'_>) -> Result<String> {
    decoder.typed_object(|decoder, tag| {
        if !tag.is_empty()
            || decoder.i64()? != 1
            || decoder.string()? != "fabric"
            || !decoder.string()?.is_empty()
            || decoder.i64()? != 0
            || !decoder.string()?.is_empty()
            || decoder.i32()? != -1
        {
            return Err(Error::Protocol("unsupported native naming URI profile"));
        }
        let path = decoder.string()?;
        if !decoder.string()?.is_empty() || !decoder.string()?.is_empty() {
            return Err(Error::Protocol("naming URI has query or fragment"));
        }
        let count = decoder.u32()? as usize;
        if count > MAX_COLLECTION_COUNT {
            return Err(Error::Protocol("naming URI segment count exceeds bound"));
        }
        let value = format!("fabric:{path}");
        crate::protocol::validate_name(&value)
            .map_err(|_| Error::Protocol("unsupported native naming URI value"))?;
        let mut segments = path[1..].split('/');
        for _ in 0..count {
            if segments.next() != Some(decoder.string()?.as_str()) {
                return Err(Error::Protocol("naming URI segments disagree with path"));
            }
        }
        if segments.next().is_some() {
            return Err(Error::Protocol(
                "naming URI segment count disagrees with path",
            ));
        }
        Ok(value)
    })
}

fn parameters(decoder: &mut Decoder<'_>) -> Result<BTreeMap<String, String>> {
    let pairs = decoder.object_array(|decoder| Ok((decoder.string()?, decoder.string()?)))?;
    let mut values = BTreeMap::new();
    for (key, value) in pairs {
        if values.insert(key, value).is_some() {
            return Err(Error::Protocol("duplicate application parameter"));
        }
    }
    Ok(values)
}

fn application(decoder: &mut Decoder<'_>) -> Result<Application> {
    let name = name(decoder)?;
    let type_name = decoder.string()?;
    let type_version = decoder.string()?;
    let status = match decoder.i64()? {
        0 => ApplicationStatus::Invalid,
        1 => ApplicationStatus::Ready,
        2 => ApplicationStatus::Upgrading,
        3 => ApplicationStatus::Creating,
        4 => ApplicationStatus::Deleting,
        5 => ApplicationStatus::Failed,
        _ => return Err(Error::Protocol("unsupported application status")),
    };
    let health_state = health(decoder)?;
    let parameters = parameters(decoder)?;
    if !decoder.is_scope_end()? && !decoder.string()?.is_empty() {
        return Err(Error::Protocol(
            "nonempty deprecated application upgrade version",
        ));
    }
    if !decoder.is_scope_end()? && !self::parameters(decoder)?.is_empty() {
        return Err(Error::Protocol(
            "nonempty deprecated application upgrade parameters",
        ));
    }
    let definition_kind = if decoder.is_scope_end()? {
        ApplicationDefinitionKind::Invalid
    } else {
        match decoder.i64()? {
            0 => ApplicationDefinitionKind::ServiceFabricApplicationDescription,
            1 => ApplicationDefinitionKind::Compose,
            2 => ApplicationDefinitionKind::MeshApplicationDescription,
            65535 => ApplicationDefinitionKind::Invalid,
            _ => return Err(Error::Protocol("unsupported application definition kind")),
        }
    };
    Ok(Application {
        name,
        type_name,
        type_version,
        status,
        health_state,
        parameters,
        definition_kind,
    })
}

fn service(decoder: &mut Decoder<'_>) -> Result<Service> {
    let raw_kind = decoder.i64()?;
    let name = name(decoder)?;
    let type_name = decoder.string()?;
    let manifest_version = decoder.string()?;
    let persisted = decoder.boolean()?;
    let kind = match (raw_kind, persisted) {
        (1, false) => ServiceKind::Stateless,
        (2, has_persisted_state) => ServiceKind::Stateful {
            has_persisted_state,
        },
        _ => return Err(Error::Protocol("unsupported service kind/persistence")),
    };
    let health_state = health(decoder)?;
    let status = if decoder.is_scope_end()? {
        ServiceStatus::Unknown
    } else {
        match decoder.i64()? {
            0 => ServiceStatus::Unknown,
            1 => ServiceStatus::Active,
            2 => ServiceStatus::Upgrading,
            3 => ServiceStatus::Deleting,
            4 => ServiceStatus::Creating,
            5 => ServiceStatus::Failed,
            _ => return Err(Error::Protocol("unsupported service status")),
        }
    };
    let is_service_group = if decoder.is_scope_end()? {
        false
    } else {
        decoder.boolean()?
    };
    Ok(Service {
        name,
        type_name,
        manifest_version,
        kind,
        health_state,
        status,
        is_service_group,
    })
}
