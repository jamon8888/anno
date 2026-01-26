use anno::backends::nuner::NuNER;
use anno::backends::span_utils::map_label_to_entity_type;
use anno::Model;
use anno_core::EntityType;

#[test]
fn label_pack_coarse_defaults() {
    let ner = NuNER::new();
    assert_eq!(ner.threshold(), 0.5);
    assert!(ner.supported_types().contains(&EntityType::Person));
}

#[test]
fn label_pack_fine_contains_expected() {
    let ner = NuNER::new().with_labels(vec![
        "animal".to_string(),
        "vehicle".to_string(),
        "person".to_string(),
        "organization".to_string(),
        "location".to_string(),
    ]);
    let labels = ner
        .supported_types()
        .into_iter()
        .map(|et| format!("{:?}", et))
        .collect::<Vec<_>>();
    assert!(labels.iter().any(|t| t.contains("ANIMAL")));
    assert!(labels.iter().any(|t| t.contains("VEHICLE")));
}

#[test]
fn label_pack_cner_contains_expected() {
    let ner = NuNER::new().with_labels(vec![
        "artifact".to_string(),
        "substance".to_string(),
        "person".to_string(),
        "organization".to_string(),
        "location".to_string(),
    ]);
    let labels = ner
        .supported_types()
        .into_iter()
        .map(|et| format!("{:?}", et))
        .collect::<Vec<_>>();
    assert!(labels.iter().any(|t| t.contains("ARTIFACT")));
    assert!(labels.iter().any(|t| t.contains("SUBSTANCE")));
}

#[test]
fn with_label_pack_overwrites_defaults() {
    let coarse = NuNER::new();
    let fine = NuNER::new().with_labels(vec![
        "animal".to_string(),
        "vehicle".to_string(),
        "person".to_string(),
        "organization".to_string(),
        "location".to_string(),
    ]);
    assert!(fine.supported_types().len() > coarse.supported_types().len());
}

#[test]
fn map_label_to_entity_type_handles_fine_labels() {
    assert!(matches!(
        map_label_to_entity_type("animal"),
        EntityType::Other(ref s) if s == "ANIMAL"
    ));
    assert!(matches!(
        map_label_to_entity_type("monetary"),
        EntityType::Money
    ));
    assert!(matches!(
        map_label_to_entity_type("number"),
        EntityType::Cardinal
    ));
}
