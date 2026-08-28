use std::{fmt, str::FromStr};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::DomainError;

const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

fn encode_uuid(uuid: Uuid) -> String {
    let mut value = uuid.as_u128();
    let mut out = [b'0'; 26];
    for slot in out.iter_mut().rev() {
        *slot = CROCKFORD[(value & 31) as usize];
        value >>= 5;
    }
    String::from_utf8(out.to_vec()).expect("Crockford alphabet is UTF-8")
}

fn decode_uuid(text: &str) -> Result<Uuid, DomainError> {
    if text.len() != 26 {
        return Err(DomainError::InvalidValue { kind: "identifier", reason: "expected 26 Crockford Base32 characters".into() });
    }
    let mut value = 0_u128;
    for (index, byte) in text.bytes().enumerate() {
        let upper = byte.to_ascii_uppercase();
        let digit = match upper {
            b'O' => Some(0), b'I' | b'L' => Some(1),
            _ => CROCKFORD.iter().position(|candidate| *candidate == upper).map(|n| n as u8),
        }.ok_or_else(|| DomainError::InvalidValue { kind: "identifier", reason: format!("invalid Crockford character at {index}") })?;
        if index == 0 && digit > 7 {
            return Err(DomainError::InvalidValue { kind: "identifier", reason: "value exceeds 128 bits".into() });
        }
        value = value.checked_mul(32).and_then(|v| v.checked_add(u128::from(digit)))
            .ok_or_else(|| DomainError::InvalidValue { kind: "identifier", reason: "value exceeds 128 bits".into() })?;
    }
    Ok(Uuid::from_u128(value))
}

macro_rules! typed_id {
    ($name:ident, $prefix:literal) => {
        #[doc = concat!("Stable UUIDv7-backed `", stringify!($name), "`.")]
        #[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub const PREFIX: &'static str = $prefix;
            #[must_use]
            pub fn new_v7() -> Self { Self(Uuid::now_v7()) }
            pub fn from_uuid(uuid: Uuid) -> Result<Self, DomainError> {
                if uuid.get_version_num() != 7 { return Err(DomainError::NotUuidV7); }
                Ok(Self(uuid))
            }
            pub fn from_bytes(bytes: [u8; 16]) -> Result<Self, DomainError> { Self::from_uuid(Uuid::from_bytes(bytes)) }
            #[must_use]
            pub const fn as_uuid(self) -> Uuid { self.0 }
            #[must_use]
            pub const fn into_bytes(self) -> [u8; 16] { *self.0.as_bytes() }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}{}", Self::PREFIX, encode_uuid(self.0)) }
        }
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { fmt::Display::fmt(self, f) }
        }
        impl FromStr for $name {
            type Err = DomainError;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                let (prefix, encoded) = value.split_once('_').ok_or_else(|| DomainError::InvalidValue { kind: stringify!($name), reason: "missing type prefix".into() })?;
                let found = format!("{prefix}_");
                if found != Self::PREFIX { return Err(DomainError::IdPrefixMismatch { expected: Self::PREFIX, found }); }
                Self::from_uuid(decode_uuid(encoded)?)
            }
        }
    };
}

typed_id!(RepoId, "repo_");
typed_id!(EventId, "evt_");
typed_id!(OperationId, "op_");
typed_id!(ActorId, "actor_");
typed_id!(StateId, "st_");
typed_id!(AttemptId, "at_");
typed_id!(BranchId, "br_");
typed_id!(WorkspaceId, "ws_");
typed_id!(CompositionId, "cmp_");
typed_id!(CandidateId, "cand_");
typed_id!(PromotionId, "prm_");
typed_id!(NavigationId, "nav_");
typed_id!(ProvenanceId, "prov_");
typed_id!(ValidationId, "ver_");
typed_id!(ArchiveId, "arc_");
typed_id!(DeltaId, "dlt_");
typed_id!(BackupId, "bak_");
typed_id!(TimeshiftId, "tsh_");
typed_id!(AnnotationId, "ann_");
typed_id!(LeaseId, "lease_");
typed_id!(HandoffId, "handoff_");
typed_id!(BoundaryId, "boundary_");
typed_id!(WorkerId, "worker_");
typed_id!(ArtifactId, "artifact_");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_ids_reject_other_prefixes() {
        let state = StateId::new_v7();
        assert!(matches!(state.to_string().parse::<AttemptId>(), Err(DomainError::IdPrefixMismatch { .. })));
        assert_eq!(state.to_string().parse::<StateId>().unwrap(), state);
    }

    #[test]
    fn constructor_rejects_non_v7_uuid() {
        assert_eq!(StateId::from_uuid(Uuid::nil()), Err(DomainError::NotUuidV7));
    }
}
