use std::{ffi::{OsStr, OsString}, fmt, str::FromStr};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use crate::error::DomainError;
use super::id::{ActorId, ArtifactId, ProvenanceId, RepoId};

#[derive(Copy, Clone, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema)]
pub struct Hash256(pub [u8; 32]);
impl Hash256 { pub const ZERO: Self = Self([0; 32]); #[must_use] pub fn digest(bytes: &[u8]) -> Self { Self(Sha256::digest(bytes).into()) } #[must_use] pub const fn as_bytes(&self) -> &[u8; 32] { &self.0 } }
impl fmt::Display for Hash256 { fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&hex::encode(self.0)) } }
impl fmt::Debug for Hash256 { fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { fmt::Display::fmt(self, f) } }
impl FromStr for Hash256 { type Err = DomainError; fn from_str(value: &str) -> Result<Self, Self::Err> { let bytes = hex::decode(value).map_err(|e| DomainError::InvalidValue { kind: "SHA-256", reason: e.to_string() })?; Ok(Self(bytes.try_into().map_err(|_| DomainError::InvalidValue { kind: "SHA-256", reason: "expected 32 bytes".into() })?)) } }

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")] pub enum ObjectAlgorithm { Sha1, Sha256 }
impl ObjectAlgorithm { #[must_use] pub const fn digest_len(&self) -> usize { match self { Self::Sha1 => 20, Self::Sha256 => 32 } } }
impl fmt::Display for ObjectAlgorithm { fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(match self { Self::Sha1 => "sha1", Self::Sha256 => "sha256" }) } }
impl FromStr for ObjectAlgorithm { type Err = DomainError; fn from_str(value: &str) -> Result<Self, Self::Err> { match value { "sha1" => Ok(Self::Sha1), "sha256" => Ok(Self::Sha256), _ => Err(DomainError::InvalidValue { kind: "Git object algorithm", reason: value.into() }) } } }

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema)]
pub struct GitObjectId { pub algorithm: ObjectAlgorithm, pub bytes: Vec<u8> }
impl GitObjectId { pub fn new(algorithm: ObjectAlgorithm, bytes: Vec<u8>) -> Result<Self, DomainError> { if bytes.len() != algorithm.digest_len() { return Err(DomainError::InvalidValue { kind: "Git object ID", reason: format!("{} requires {} bytes, found {}", algorithm, algorithm.digest_len(), bytes.len()) }); } Ok(Self { algorithm, bytes }) } pub fn from_hex(algorithm: ObjectAlgorithm, value: &str) -> Result<Self, DomainError> { Self::new(algorithm, hex::decode(value).map_err(|e| DomainError::InvalidValue { kind: "Git object ID", reason: e.to_string() })?) } #[must_use] pub fn hex(&self) -> String { hex::encode(&self.bytes) } }
impl fmt::Display for GitObjectId { fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}:{}", self.algorithm, self.hex()) } }
impl fmt::Debug for GitObjectId { fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { fmt::Display::fmt(self, f) } }
impl FromStr for GitObjectId { type Err = DomainError; fn from_str(value: &str) -> Result<Self, Self::Err> { let (algorithm, hex) = value.split_once(':').ok_or_else(|| DomainError::InvalidValue { kind: "Git object ID", reason: "expected algorithm:hex".into() })?; Self::from_hex(algorithm.parse()?, hex) } }

macro_rules! validated_text { ($name:ident, $kind:literal) => { #[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema)] #[serde(transparent)] pub struct $name(String); impl $name { pub fn new(value: impl Into<String>) -> Result<Self, DomainError> { let value = value.into(); if value.trim().is_empty() || value.contains('\0') { return Err(DomainError::InvalidValue { kind: $kind, reason: "must be non-empty and contain no NUL".into() }); } Ok(Self(value)) } #[must_use] pub fn as_str(&self) -> &str { &self.0 } } impl fmt::Display for $name { fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.0) } } impl FromStr for $name { type Err = DomainError; fn from_str(value: &str) -> Result<Self, Self::Err> { Self::new(value) } } } }
validated_text!(JjChangeId, "JJ change ID"); validated_text!(JjCommitId, "JJ commit ID"); validated_text!(JjOperationId, "JJ operation ID");

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "encoding", content = "units", rename_all = "kebab-case")] pub enum NativePath { UnixBytes(Vec<u8>), WindowsWide(Vec<u16>) }
impl NativePath { pub fn unix(bytes: Vec<u8>) -> Result<Self, DomainError> { if bytes.contains(&0) { return Err(DomainError::InvalidValue { kind: "native path", reason: "contains NUL".into() }); } Ok(Self::UnixBytes(bytes)) } pub fn windows(wide: Vec<u16>) -> Result<Self, DomainError> { if wide.contains(&0) { return Err(DomainError::InvalidValue { kind: "native path", reason: "contains NUL".into() }); } Ok(Self::WindowsWide(wide)) } #[must_use] pub fn display_lossy(&self) -> String { match self { Self::UnixBytes(bytes) => String::from_utf8_lossy(bytes).into_owned(), Self::WindowsWide(wide) => String::from_utf16_lossy(wide) } } #[cfg(unix)] pub fn from_os_str(value: &OsStr) -> Result<Self, DomainError> { use std::os::unix::ffi::OsStrExt; Self::unix(value.as_bytes().to_vec()) } #[cfg(unix)] pub fn to_os_string(&self) -> Result<OsString, DomainError> { use std::os::unix::ffi::OsStringExt; match self { Self::UnixBytes(bytes) => Ok(OsString::from_vec(bytes.clone())), Self::WindowsWide(_) => Err(DomainError::InvalidValue { kind: "native path", reason: "Windows path on Unix".into() }) } } #[cfg(windows)] pub fn from_os_str(value: &OsStr) -> Result<Self, DomainError> { use std::os::windows::ffi::OsStrExt; Self::windows(value.encode_wide().collect()) } #[cfg(windows)] pub fn to_os_string(&self) -> Result<OsString, DomainError> { use std::os::windows::ffi::OsStringExt; match self { Self::WindowsWide(wide) => Ok(OsString::from_wide(wide)), Self::UnixBytes(_) => Err(DomainError::InvalidValue { kind: "native path", reason: "Unix path on Windows".into() }) } } }

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema)]
pub struct RepositoryFingerprint { pub repo_id: RepoId, pub common_dir_identity: Hash256, pub object_format: ObjectAlgorithm, pub head: Option<GitObjectId>, pub refs_digest: Hash256, pub index_digest: Hash256, pub worktree_digest: Hash256 }
impl RepositoryFingerprint { #[must_use] pub fn digest(&self) -> Hash256 { Hash256::digest(&serde_json::to_vec(self).expect("serializing domain value cannot fail")) } }
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema)] #[serde(rename_all = "kebab-case")] pub enum ActorKind { Human, Agent, System, Import }
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema)] pub struct ActorRef { pub id: ActorId, pub kind: ActorKind, pub display_name: Option<String> }
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema)] pub struct ArtifactRef { pub id: ArtifactId, pub sha256: Hash256, pub byte_length: u64, pub media_type: String }
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema)] pub struct Provenance { pub id: ProvenanceId, pub algorithm: String, pub source: String, pub source_digest: Hash256, pub details: Vec<(String, String)> }
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema)] #[serde(transparent)] pub struct UtcTimestamp(String);
impl UtcTimestamp { pub fn parse(value: impl Into<String>) -> Result<Self, DomainError> { let value = value.into(); let parsed = time::OffsetDateTime::parse(&value, &time::format_description::well_known::Rfc3339).map_err(|e| DomainError::InvalidValue { kind: "UTC timestamp", reason: e.to_string() })?; if parsed.offset() != time::UtcOffset::UTC { return Err(DomainError::InvalidValue { kind: "UTC timestamp", reason: "offset must be Z".into() }); } Ok(Self(value)) } #[must_use] pub fn as_str(&self) -> &str { &self.0 } }

#[cfg(test)] mod tests { use super::*; #[test] fn git_object_ids_are_algorithm_qualified() { let oid = GitObjectId::from_hex(ObjectAlgorithm::Sha1, &"ab".repeat(20)).unwrap(); assert_eq!(oid.to_string().parse::<GitObjectId>().unwrap(), oid); assert!(GitObjectId::from_hex(ObjectAlgorithm::Sha256, &"ab".repeat(20)).is_err()); } #[cfg(unix)] #[test] fn native_path_round_trips_non_utf8() { use std::os::unix::ffi::OsStringExt; let original = OsString::from_vec(vec![b'a', 0xff]); let value = NativePath::from_os_str(&original).unwrap(); assert_eq!(value.to_os_string().unwrap(), original); } }
