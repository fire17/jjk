use std::collections::BTreeMap;
use schemars::JsonSchema;
use serde::{Deserialize,Serialize};
use crate::error::DomainError;
use super::{attempt::Attempt,evidence::{EvidenceRef,ValidationRecord},graph::{GraphEdge,StateGraph},id::*,operation::{EffectReceipt,OperationPhase,OperationPlan,OperationReceipt},provenance::*,state::{Annotation,State},workspace::Workspace};

#[derive(Copy,Clone,Debug,Eq,PartialEq,Ord,PartialOrd,Hash,Serialize,Deserialize,JsonSchema)] #[serde(rename_all="kebab-case")] pub enum PayloadCodec{CanonicalJsonV1}
#[derive(Copy,Clone,Debug,Eq,PartialEq,Ord,PartialOrd,Hash,Serialize,Deserialize,JsonSchema)] pub enum EventType { SafeSpaceInitialized,OperationPrepared,ApplyStarted,EffectObserved,ConflictPaused,VerificationStarted,OperationCommitted,AbortStarted,OperationAborted,RepairRequired,RepairResumed,GitCommitObserved,GitRefObserved,JjOperationObserved,WorkspaceObserved,StateCaptured,StateAnnotated,StateActivated,AttemptForked,DeltaApplied,ValidationRecorded,CanonicalPromoted,StateArchived,StateRecovered,BackupCreated,RestorePrepared,RestoreApplied,MigrationStarted,MigrationCompleted,MigrationFailed }
#[derive(Clone,Debug,Eq,PartialEq,Serialize,Deserialize,JsonSchema)] #[serde(tag="type",content="data")] pub enum EventV1 {
 SafeSpaceInitialized{capabilities_digest:Hash256},OperationPrepared(OperationPlan),ApplyStarted{effect_ids:Vec<super::operation::EffectId>},EffectObserved(EffectReceipt),ConflictPaused{code:String,options:Vec<String>},VerificationStarted{expected:RepositoryFingerprint},OperationCommitted(OperationReceipt),AbortStarted{reason:String},OperationAborted(OperationReceipt),RepairRequired{reason:String},RepairResumed{strategy:String},GitCommitObserved{oid:GitObjectId,parents:Vec<GitObjectId>,tree:GitObjectId},GitRefObserved{branch_id:BranchId,refname:Vec<u8>,target:GitObjectId},JjOperationObserved{operation:JjOperationId},WorkspaceObserved(Workspace),StateCaptured(State),StateAnnotated(Annotation),StateActivated{navigation_id:NavigationId,state_id:StateId,workspace_id:WorkspaceId,prior_state_id:Option<StateId>},AttemptForked(Attempt),DeltaApplied{composition_id:CompositionId,delta_id:DeltaId,source_state_id:StateId,source_parent_state_id:StateId,target_base_state_id:StateId,result_state_id:StateId,patch:ArtifactRef},ValidationRecorded(ValidationRecord),CanonicalPromoted{promotion_id:PromotionId,source_state_id:StateId,previous_state_id:Option<StateId>,validations:Vec<ValidationId>,resulting_ref_oid:GitObjectId},StateArchived{state_id:StateId,archive_id:ArchiveId,reason:String},StateRecovered{state_id:StateId,archive_id:ArchiveId},BackupCreated{backup_id:BackupId,through_seq:u64,through_hash:Hash256,manifest:ArtifactRef},RestorePrepared{backup_id:BackupId,pre_restore_backup_id:BackupId},RestoreApplied{backup_id:BackupId,result_fingerprint:RepositoryFingerprint},MigrationStarted{from:u32,to:u32,backup_id:BackupId},MigrationCompleted{from:u32,to:u32,tool_version:String},MigrationFailed{from:u32,to:u32,error:ArtifactRef}
}
impl EventV1 { #[must_use] pub const fn event_type(&self)->EventType{match self{Self::SafeSpaceInitialized{..}=>EventType::SafeSpaceInitialized,Self::OperationPrepared(_)=>EventType::OperationPrepared,Self::ApplyStarted{..}=>EventType::ApplyStarted,Self::EffectObserved(_)=>EventType::EffectObserved,Self::ConflictPaused{..}=>EventType::ConflictPaused,Self::VerificationStarted{..}=>EventType::VerificationStarted,Self::OperationCommitted(_)=>EventType::OperationCommitted,Self::AbortStarted{..}=>EventType::AbortStarted,Self::OperationAborted(_)=>EventType::OperationAborted,Self::RepairRequired{..}=>EventType::RepairRequired,Self::RepairResumed{..}=>EventType::RepairResumed,Self::GitCommitObserved{..}=>EventType::GitCommitObserved,Self::GitRefObserved{..}=>EventType::GitRefObserved,Self::JjOperationObserved{..}=>EventType::JjOperationObserved,Self::WorkspaceObserved(_)=>EventType::WorkspaceObserved,Self::StateCaptured(_)=>EventType::StateCaptured,Self::StateAnnotated(_)=>EventType::StateAnnotated,Self::StateActivated{..}=>EventType::StateActivated,Self::AttemptForked(_)=>EventType::AttemptForked,Self::DeltaApplied{..}=>EventType::DeltaApplied,Self::ValidationRecorded(_)=>EventType::ValidationRecorded,Self::CanonicalPromoted{..}=>EventType::CanonicalPromoted,Self::StateArchived{..}=>EventType::StateArchived,Self::StateRecovered{..}=>EventType::StateRecovered,Self::BackupCreated{..}=>EventType::BackupCreated,Self::RestorePrepared{..}=>EventType::RestorePrepared,Self::RestoreApplied{..}=>EventType::RestoreApplied,Self::MigrationStarted{..}=>EventType::MigrationStarted,Self::MigrationCompleted{..}=>EventType::MigrationCompleted,Self::MigrationFailed{..}=>EventType::MigrationFailed}} }
#[derive(Clone,Debug,Eq,PartialEq,Serialize,Deserialize,JsonSchema)] pub struct EventHeaderV1 { pub event_id:EventId,pub repo_id:RepoId,pub local_seq:u64,pub operation_id:OperationId,pub operation_ordinal:u32,pub actor:ActorRef,pub recorded_at_utc:UtcTimestamp,pub observed_at_utc:Option<UtcTimestamp>,pub repository_fingerprint:RepositoryFingerprint,pub provenance:Provenance,pub evidence:Vec<EvidenceRef>,pub dedup_key:Option<String>,pub previous_event_hash:Hash256 }
#[derive(Clone,Debug,Eq,PartialEq,Serialize,Deserialize,JsonSchema)] pub struct EventEnvelopeV1 { pub event_id:EventId,pub repo_id:RepoId,pub local_seq:u64,pub event_type:EventType,pub event_schema_version:u16,pub envelope_version:u16,pub operation_id:OperationId,pub operation_ordinal:u32,pub actor:ActorRef,pub recorded_at_utc:UtcTimestamp,pub observed_at_utc:Option<UtcTimestamp>,pub repository_fingerprint:RepositoryFingerprint,pub payload_codec:PayloadCodec,pub payload:EventV1,pub provenance:Provenance,pub evidence:Vec<EvidenceRef>,pub dedup_key:Option<String>,pub previous_event_hash:Hash256,pub event_hash:Hash256 }
impl EventEnvelopeV1 {
    pub fn new(header: EventHeaderV1, payload: EventV1) -> Result<Self, DomainError> {
        let mut value = Self { event_id: header.event_id, repo_id: header.repo_id, local_seq: header.local_seq, event_type: payload.event_type(), event_schema_version: 1, envelope_version: 1, operation_id: header.operation_id, operation_ordinal: header.operation_ordinal, actor: header.actor, recorded_at_utc: header.recorded_at_utc, observed_at_utc: header.observed_at_utc, repository_fingerprint: header.repository_fingerprint, payload_codec: PayloadCodec::CanonicalJsonV1, payload, provenance: header.provenance, evidence: header.evidence, dedup_key: header.dedup_key, previous_event_hash: header.previous_event_hash, event_hash: Hash256::ZERO };
        value.event_hash = value.recompute_hash();
        Ok(value)
    }
    #[must_use]
    pub fn canonical_hash_input(&self) -> Vec<u8> {
        let mut clone = self.clone();
        clone.event_hash = Hash256::ZERO;
        let mut bytes = b"jjk-event-v1".to_vec();
        bytes.extend(serde_json::to_vec(&clone).expect("serialize event"));
        bytes
    }
    #[must_use] pub fn recompute_hash(&self) -> Hash256 { Hash256::digest(&self.canonical_hash_input()) }
    pub fn verify_hash(&self) -> Result<(), DomainError> {
        if self.event_type != self.payload.event_type() { return Err(DomainError::EventTypeMismatch); }
        if self.recompute_hash() != self.event_hash { return Err(DomainError::EventHashMismatch { sequence: self.local_seq }); }
        Ok(())
    }
}

#[derive(Clone,Debug,Default,Eq,PartialEq,Serialize,Deserialize,JsonSchema)] pub struct ReferenceProjection { pub graph:StateGraph,pub operations:BTreeMap<OperationId,OperationPhase>,pub validations:BTreeMap<ValidationId,ValidationRecord>,pub last_event_seq:u64,pub last_event_hash:Hash256 }
impl ReferenceProjection { pub fn replay<'a>(events:impl IntoIterator<Item=&'a EventEnvelopeV1>)->Result<Self,DomainError>{let mut projection=Self::default();for event in events{projection.apply(event)?;}projection.graph.validate()?;Ok(projection)} pub fn apply(&mut self,event:&EventEnvelopeV1)->Result<(),DomainError>{let expected=self.last_event_seq+1;if event.local_seq!=expected{return Err(DomainError::EventSequenceGap{expected,found:event.local_seq})}if event.previous_event_hash!=self.last_event_hash||event.verify_hash().is_err(){return Err(DomainError::EventHashMismatch{sequence:event.local_seq})}match &event.payload{EventV1::OperationPrepared(_)=>{self.operations.insert(event.operation_id,OperationPhase::Prepared);},EventV1::ApplyStarted{..}=>self.advance(event.operation_id,super::operation::OperationTransition::ApplyStarted)?,EventV1::EffectObserved(_)=>self.advance(event.operation_id,super::operation::OperationTransition::EffectObserved)?,EventV1::ConflictPaused{..}=>self.advance(event.operation_id,super::operation::OperationTransition::ConflictPaused)?,EventV1::VerificationStarted{..}=>self.advance(event.operation_id,super::operation::OperationTransition::VerificationStarted)?,EventV1::OperationCommitted(_)=>self.advance(event.operation_id,super::operation::OperationTransition::Committed)?,EventV1::AbortStarted{..}=>self.advance(event.operation_id,super::operation::OperationTransition::AbortStarted)?,EventV1::OperationAborted(_)=>self.advance(event.operation_id,super::operation::OperationTransition::Aborted)?,EventV1::RepairRequired{..}=>self.advance(event.operation_id,super::operation::OperationTransition::RepairRequired)?,EventV1::AttemptForked(attempt)=>self.graph.add_attempt(attempt.clone())?,EventV1::StateCaptured(state)=>self.graph.add_state(state.clone())?,EventV1::ValidationRecorded(record)=>{if self.validations.insert(record.id,record.clone()).is_some(){return Err(DomainError::Duplicate{kind:"validation",id:record.id.to_string()})}},_=>{}}self.last_event_seq=event.local_seq;self.last_event_hash=event.event_hash;Ok(())} fn advance(&mut self,id:OperationId,transition:super::operation::OperationTransition)->Result<(),DomainError>{let phase=*self.operations.get(&id).ok_or_else(||DomainError::Missing{kind:"operation",id:id.to_string()})?;self.operations.insert(id,phase.transition(transition)?);Ok(())} #[must_use] pub fn canonical_bytes(&self)->Vec<u8>{serde_json::to_vec(self).expect("serialize projection")} #[must_use] pub fn digest(&self)->Hash256{Hash256::digest(&self.canonical_bytes())} }

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture_header(seq: u64, previous: Hash256, op: OperationId) -> EventHeaderV1 {
        let repo = RepoId::new_v7();
        EventHeaderV1 { event_id: EventId::new_v7(), repo_id: repo, local_seq: seq, operation_id: op, operation_ordinal: (seq - 1) as u32, actor: ActorRef { id: ActorId::new_v7(), kind: ActorKind::System, display_name: None }, recorded_at_utc: UtcTimestamp::parse("2026-08-28T00:00:00Z").unwrap(), observed_at_utc: None, repository_fingerprint: RepositoryFingerprint { repo_id: repo, common_dir_identity: Hash256::ZERO, object_format: ObjectAlgorithm::Sha1, head: None, refs_digest: Hash256::ZERO, index_digest: Hash256::ZERO, worktree_digest: Hash256::ZERO }, provenance: Provenance { id: ProvenanceId::new_v7(), algorithm: "fixture".into(), source: "test".into(), source_digest: Hash256::ZERO, details: vec![] }, evidence: vec![], dedup_key: None, previous_event_hash: previous }
    }
    #[test]
    fn event_hash_is_deterministic() {
        let op = OperationId::new_v7();
        let event = EventEnvelopeV1::new(fixture_header(1, Hash256::ZERO, op), EventV1::SafeSpaceInitialized { capabilities_digest: Hash256::ZERO }).unwrap();
        assert_eq!(event.event_hash, event.recompute_hash());
        let decoded: EventEnvelopeV1 = serde_json::from_slice(&serde_json::to_vec(&event).unwrap()).unwrap();
        assert_eq!(event.event_hash, decoded.recompute_hash());
    }
    #[test]
    fn reducer_replay_is_deterministic() {
        let op = OperationId::new_v7();
        let event = EventEnvelopeV1::new(fixture_header(1, Hash256::ZERO, op), EventV1::SafeSpaceInitialized { capabilities_digest: Hash256::ZERO }).unwrap();
        let a = ReferenceProjection::replay([&event]).unwrap();
        let b = ReferenceProjection::replay([&event]).unwrap();
        assert_eq!(a.digest(), b.digest());
        assert!(ReferenceProjection::replay([&event, &event]).is_err());
    }
}
