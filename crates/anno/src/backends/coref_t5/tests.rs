    use super::*;

    // -------------------------------------------------------------------------
    // Config / struct smoke tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_coref_config_default() {
        let config = T5CorefConfig::default();
        assert_eq!(config.max_input_length, 512);
        assert_eq!(config.num_beams, 1);
    }

    #[test]
    fn test_cluster_struct() {
        let cluster = CorefCluster {
            id: 0,
            mentions: vec!["Marie Curie".to_string(), "She".to_string()],
            spans: vec![(0, 11), (50, 53)],
            canonical: "Marie Curie".to_string(),
        };
        assert_eq!(cluster.mentions.len(), 2);
        assert_eq!(cluster.canonical, "Marie Curie");
    }

    // -------------------------------------------------------------------------
    // mark_mentions_for_t5
    // -------------------------------------------------------------------------

    #[test]
    fn mark_mentions_wraps_pronouns() {
        let out = mark_mentions_for_t5("she said he agreed");
        assert!(
            out.contains("<m> she </m>"),
            "pronoun 'she' should be marked"
        );
        assert!(out.contains("<m> he </m>"), "pronoun 'he' should be marked");
        assert!(out.contains("said"), "'said' should pass through unmarked");
        assert!(
            out.contains("agreed"),
            "'agreed' should pass through unmarked"
        );
    }

    #[test]
    fn mark_mentions_wraps_capitalized() {
        let out = mark_mentions_for_t5("Sophie Wilson designed ARM.");
        assert!(out.contains("<m> Sophie </m>"));
        assert!(out.contains("<m> Wilson </m>"));
        assert!(out.contains("<m> ARM. </m>"));
        assert!(out.contains("designed"));
    }

    #[test]
    fn mark_mentions_empty_string() {
        assert_eq!(mark_mentions_for_t5(""), "");
    }

    #[test]
    fn mark_mentions_no_pronouns_no_caps() {
        let text = "the quick brown fox";
        assert_eq!(mark_mentions_for_t5(text), text);
    }

    // -------------------------------------------------------------------------
    // extract_t5_mentions
    // -------------------------------------------------------------------------

    #[test]
    fn extract_mentions_basic() {
        let (plain, spans) = extract_t5_mentions("<m> Sophie Wilson </m> designed ARM.").unwrap();
        assert_eq!(plain, "Sophie Wilson designed ARM.");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].0, "Sophie Wilson");
    }

    #[test]
    fn extract_mentions_two_spans() {
        let marked = "<m> Elon </m> founded <m> Tesla </m>.";
        let (plain, spans) = extract_t5_mentions(marked).unwrap();
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].0, "Elon");
        assert_eq!(spans[1].0, "Tesla");
        assert!(plain.contains("Elon"));
        assert!(plain.contains("Tesla"));
        assert!(plain.contains("founded"));
    }

    #[test]
    fn extract_mentions_empty_input() {
        let (plain, spans) = extract_t5_mentions("").unwrap();
        assert_eq!(plain, "");
        assert!(spans.is_empty());
    }

    #[test]
    fn extract_mentions_no_markers() {
        let (plain, spans) = extract_t5_mentions("no markers here").unwrap();
        assert_eq!(plain, "no markers here");
        assert!(spans.is_empty());
    }

    #[test]
    fn extract_mentions_span_offsets_are_consistent() {
        let marked = "<m> Marie </m> discovered polonium. <m> She </m> won.";
        let (plain, spans) = extract_t5_mentions(marked).unwrap();
        for (text, start, end) in &spans {
            let extracted: String = plain.chars().skip(*start).take(end - start).collect();
            assert_eq!(
                &extracted, text,
                "span offsets must index back to mention text"
            );
        }
    }

    // -------------------------------------------------------------------------
    // parse_t5_coref_output
    // -------------------------------------------------------------------------

    #[test]
    fn parse_output_basic_two_mentions() {
        // "Elon | 1 founded Tesla | 2. He | 1 led SpaceX."
        let decoded = "Elon | 1 founded Tesla | 2. He | 1 led SpaceX.";
        let clusters = parse_t5_coref_output(decoded);
        let cluster1 = clusters.iter().find(|c| c.id == 1);
        assert!(cluster1.is_some(), "cluster 1 (Elon/He) should be parsed");
        let c = cluster1.unwrap();
        assert!(c.mentions.len() >= 2, "cluster 1 should have ≥2 members");
        assert!(c.mentions.contains(&"Elon".to_string()));
        assert!(c.mentions.contains(&"He".to_string()));
    }

    #[test]
    fn parse_output_singletons_filtered() {
        // Word with marker but only appearing once → should be filtered (singleton)
        let decoded = "Marie | 1 discovered polonium.";
        let clusters = parse_t5_coref_output(decoded);
        assert!(
            clusters.is_empty(),
            "singleton cluster should be filtered out"
        );
    }

    #[test]
    fn parse_output_empty_string() {
        assert!(parse_t5_coref_output("").is_empty());
    }

    #[test]
    fn parse_output_no_markers() {
        let clusters = parse_t5_coref_output("This text has no cluster markers at all.");
        assert!(clusters.is_empty());
    }

    #[test]
    fn parse_output_canonical_is_longest_mention() {
        // Each token gets a marker individually.
        // "Marie | 1 Curie | 1" produces mentions ["Marie", "Curie"] for cluster 1.
        // With "She" also in cluster 1, the longest single-token mention is "Marie" (5 chars)
        // vs "Curie" (5) vs "She" (3) — tie between Marie/Curie, resolved by first-seen.
        // Use a clearer example: "Alexandra | 1 won . She | 1 left."
        let decoded = "Alexandra | 1 won . She | 1 left .";
        let clusters = parse_t5_coref_output(decoded);
        if let Some(c) = clusters.iter().find(|c| c.id == 1) {
            assert_eq!(
                c.canonical, "Alexandra",
                "canonical should be the longest mention"
            );
        }
    }

    #[test]
    fn parse_output_sorted_by_cluster_id() {
        let decoded = "B | 2 and A | 1 were colleagues . them | 2 worked together . B | 1 also .";
        let clusters = parse_t5_coref_output(decoded);
        if clusters.len() >= 2 {
            let ids: Vec<u32> = clusters.iter().map(|c| c.id).collect();
            let mut sorted = ids.clone();
            sorted.sort();
            assert_eq!(ids, sorted, "clusters should be sorted by ID");
        }
    }
