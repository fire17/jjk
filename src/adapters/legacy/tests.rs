use super::repo_json::*;
use std::collections::BTreeSet;
use std::fs;
use tempfile::tempdir;

struct Objects(BTreeSet<String>);
impl GitObjectLookup for Objects {
    fn object_exists(&self, oid: &str) -> Result<bool, String> {
        Ok(self.0.contains(oid))
    }
}
#[derive(Default)]
struct Sink {
    receipt: Option<LegacyMigrationReceipt>,
    mappings: usize,
    entities: usize,
}
impl LegacyImportSink for Sink {
    fn existing_receipt(&mut self, _: &str) -> Result<Option<LegacyMigrationReceipt>, String> {
        Ok(self.receipt.clone())
    }
    fn begin_import(&mut self, _: &LegacyImportPlan) -> Result<(), String> {
        Ok(())
    }
    fn put_id_mapping(&mut self, _: &LegacyIdMapEntry) -> Result<(), String> {
        self.mappings += 1;
        Ok(())
    }
    fn put_entity(&mut self, _: &ImportedEntity) -> Result<(), String> {
        self.entities += 1;
        Ok(())
    }
    fn commit_import(&mut self, receipt: &LegacyMigrationReceipt) -> Result<(), String> {
        self.receipt = Some(receipt.clone());
        Ok(())
    }
    fn abort_import(&mut self) {}
}

#[test]
fn complete_fixture_is_idempotent_and_preserves_source_bytes() {
    let temp = tempdir().unwrap();
    let jjk = temp.path().join(".jjk");
    fs::create_dir(&jjk).unwrap();
    let source = include_bytes!("../../../migrations/fixtures/legacy-v1-complete/repo.json");
    fs::write(jjk.join("repo.json"), source).unwrap();
    let oid = "1111111111111111111111111111111111111111".to_owned();
    let plan = LegacyImportPlan::discover(temp.path(), &Objects(BTreeSet::from([oid]))).unwrap();
    let capsule = jjk.join("migrations/legacy-v1/test");
    plan.preserve_sources(&capsule).unwrap();
    assert_eq!(fs::read(jjk.join("repo.json")).unwrap(), source);
    assert_eq!(fs::read(capsule.join("repo.json")).unwrap(), source);
    let stable = plan.id_map.clone();
    let again = LegacyImportPlan::discover(
        temp.path(),
        &Objects(BTreeSet::from([
            "1111111111111111111111111111111111111111".into()
        ])),
    )
    .unwrap();
    assert_eq!(stable, again.id_map);
    let mut sink = Sink::default();
    let first = plan.apply(&mut sink).unwrap();
    let second = plan.apply(&mut sink).unwrap();
    assert!(!first.already_imported);
    assert!(second.already_imported);
    assert_eq!(sink.entities, plan.entities.len());
}

#[test]
fn preserved_capsule_is_immutable_and_rejects_extra_or_changed_files() {
    let temp = tempdir().unwrap();
    let jjk = temp.path().join(".jjk");
    fs::create_dir(&jjk).unwrap();
    let source = include_bytes!("../../../migrations/fixtures/legacy-v1-complete/repo.json");
    fs::write(jjk.join("repo.json"), source).unwrap();
    let oid = "1111111111111111111111111111111111111111".to_owned();
    let plan = LegacyImportPlan::discover(temp.path(), &Objects(BTreeSet::from([oid]))).unwrap();
    let capsule = temp.path().join("control/migrations/legacy-v1/receipt");
    plan.preserve_sources(&capsule).unwrap();
    fs::write(capsule.join("repo.json"), b"changed").unwrap();
    assert!(matches!(
        plan.preserve_sources(&capsule),
        Err(LegacyImportError::PreserveDigest(_))
    ));
    assert_eq!(fs::read(jjk.join("repo.json")).unwrap(), source);
}

#[test]
fn missing_object_blocks_import_and_unsafe_timeshift_is_quarantined() {
    let temp = tempdir().unwrap();
    let jjk = temp.path().join(".jjk");
    fs::create_dir(&jjk).unwrap();
    let mut value: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../migrations/fixtures/legacy-v1-complete/repo.json"
    ))
    .unwrap();
    value["timeshifts"][0]["relativeCwd"] = "../escape".into();
    fs::write(jjk.join("repo.json"), serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(matches!(
        LegacyImportPlan::discover(temp.path(), &Objects(BTreeSet::new())),
        Err(LegacyImportError::MissingGitObjects { .. })
    ));
    let plan = LegacyImportPlan::discover(
        temp.path(),
        &Objects(BTreeSet::from([
            "1111111111111111111111111111111111111111".into()
        ])),
    )
    .unwrap();
    assert_eq!(plan.quarantined.len(), 1);
    assert!(
        !plan
            .entities
            .iter()
            .any(|entity| entity.entity_kind == "timeshift")
    );
}
