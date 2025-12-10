<!-- Auto-generated from dataset_registry.rs - DO NOT EDIT MANUALLY -->
<!-- Run `cargo test generate_datasets_markdown -- --ignored` to regenerate -->

# Dataset Registry

**Total datasets: 431**

## Coverage Summary

| Category | Count |
|----------|-------|
| NER | 283 |
| Coreference | 76 |
| Event Coref (CDCR) | 13 |
| Abstract Anaphora | 7 |
| Biomedical | 45 |
| Multilingual | 76 |
| Historical | 29 |
| Ancient Languages | 20 |
| Indigenous | 10 |
| Low-Resource | 36 |
| Literary | 22 |
| Relation Extraction | 38 |
| Nested NER | 13 |
| Arcane Domains | 112 |
| Adversarial | 12 |
| Constructed Languages | 16 |
| Dialogue/Conversational | 18 |

## All Datasets

| ID | Name | Language | Domain | License | Categories |
|----|------|----------|--------|---------|------------|
| `WikiGold` | WikiGold | en | wikipedia | CC-BY-4.0 | ner |
| `Wnut17` | WNUT-17 | en | social_media | CC-BY-4.0 | ner, social_media |
| `MitMovie` | MIT Movie | en | entertainment | Research | ner |
| `MitRestaurant` | MIT Restaurant | en | restaurant | Research | ner |
| `CoNLL2003Sample` | CoNLL-2003 Sample | en | news | Research | ner |
| `OntoNotesSample` | OntoNotes Sample | en | news | LDC | ner |
| `MultiNERD` | MultiNERD | en | wikipedia | CC-BY-SA-4.0 | ner, multilingual |
| `FewNERD` | Few-NERD | en | wikipedia | CC-BY-SA-4.0 | ner |
| `CrossNER` | CrossNER | en | multi-domain | MIT | ner |
| `FabNER` | FabNER | en | manufacturing | CC-BY-4.0 | ner |
| `BroadTwitterCorpus` | Broad Twitter Corpus | en | social_media | CC-BY-4.0 | ner, social_media |
| `WikiNeural` | WikiNeural | multi | wikipedia | CC-BY-SA-4.0 | ner, multilingual |
| `PolyglotNER` | Polyglot-NER | multi | wikipedia | Research | ner, multilingual |
| `UniversalNERBench` | Universal NER | multi | mixed | CC-BY-4.0 | ner, multilingual |
| `CoNLL2002` | CoNLL-2002 | multi | news | Research | ner, multilingual |
| `TweetNER7` | TweetNER7 | en | social_media | CC-BY-4.0 | ner, social_media |
| `GoogleRE` | Google-RE | en | wikipedia | CC-BY-4.0 | relation_extraction |
| `NYTFB` | NYT-FB | en | news | Research | relation_extraction |
| `REBEL` | REBEL | en | wikipedia | CC-BY-SA-4.0 | relation_extraction |
| `MultiCoNER` | MultiCoNER | multi | mixed | CC-BY-4.0 | ner, multilingual |
| `MultiCoNERv2` | MultiCoNER v2 | multi | mixed | CC-BY-4.0 | ner, multilingual |
| `BC5CDR` | BC5CDR | en | biomedical | Public | ner, biomedical |
| `NCBIDisease` | NCBI Disease | en | biomedical | Public | ner, biomedical |
| `GENIA` | GENIA | en | biomedical | GENIA Project License | ner, biomedical |
| `AnatEM` | AnatEM | en | biomedical | CC-BY-4.0 | ner, biomedical |
| `BC2GM` | BC2GM | en | biomedical | Research | ner, biomedical |
| `BC4CHEMD` | BC4CHEMD | en | biomedical | Research | ner, biomedical |
| `GAP` | GAP | en | wikipedia | Apache-2.0 | coref, bias_evaluation |
| `PreCo` | PreCo | en | general | CC-BY-4.0 | coref |
| `LitBank` | LitBank | en | literature | CC-BY-4.0 | coref, literary |
| `ECBPlus` | ECB+ | en | news | CC-BY-3.0 | coref, event_coref |
| `OntoNotesCoref` | OntoNotes Coreference | en | mixed | LDC | coref |
| `WikiCoref` | WikiCoref | en | wikipedia | CC-BY-SA-4.0 | coref |
| `ARRAU3` | ARRAU 3.0 | en | mixed | Research | coref |
| `AMIMeeting` | AMI Meeting | en | dialogue | CC-BY-4.0 | coref, dialogue |
| `CLEFClinicalCoref` | CLEF Clinical Coreference | en | clinical | PhysioNet | coref, biomedical |
| `RSTDT` | RST Discourse Treebank | en | news | LDC | coref |
| `WinoBias` | WinoBias | en | evaluation | MIT | coref, bias_evaluation |
| `QxoRef` | qxoRef | qxo | narrative | CC-BY-NC-SA-4.0 | coref, indigenous, low_resource |
| `AmericasNLI` | AmericasNLI | multi | general | CC-BY-4.0 | multilingual, indigenous, low_resource |
| `CherokeeNER` | Cherokee NER | chr | general | Research | ner, indigenous, low_resource |
| `NahuatlNER` | Nahuatl NER | nah | historical | CC-BY-4.0 | ner, historical, indigenous, low_resource |
| `MaoriNER` | Māori NER | mi | general | Research | ner, indigenous, low_resource |
| `WelshNER` | Welsh NER | cy | news | CC-BY-4.0 | ner, indigenous, low_resource |
| `BasqueNER` | Basque NER | eu | news | CC-BY-4.0 | ner, indigenous, low_resource |
| `HIPE2022` | HIPE-2022 | multi | historical | CC-BY-NC-4.0 | ner, multilingual, historical |
| `HistNERo` | HistNERo | ro | historical | CC-BY-4.0 | ner, historical, low_resource |
| `QuaeroOldPress` | Quaero Old Press | fr | historical | Research | ner, historical |
| `HistoricalChineseNER` | Historical Chinese NER | zh | historical | Research | ner, coref, multilingual, historical, entity_linking |
| `CHisIEC` | CHisIEC | lzh | historical | Research | ner, historical, relation_extraction, ancient |
| `DocRED` | DocRED | en | wikipedia | MIT | relation_extraction |
| `ReTACRED` | Re-TACRED | en | news | LDC | relation_extraction |
| `ACE2004` | ACE 2004 | en | news | LDC | ner, nested_ner |
| `CADEC` | CADEC | en | biomedical | Research | ner, biomedical, discontinuous_ner |
| `WinoQueer` | WinoQueer | en | evaluation | MIT | bias_evaluation |
| `BBQ` | BBQ | en | evaluation | CC-BY-4.0 | bias_evaluation |
| `GICoref` | GICoref | en | evaluation | CC-BY-4.0 | coref, bias_evaluation |
| `CorefUD` | CorefUD | multi | general | CC-BY-NC-SA-4.0 | coref, multilingual |
| `TransMuCoRes` | TransMuCoRes | multi | general | Research | coref, multilingual |
| `MGAP` | mGAP | multi | evaluation | Research | coref, multilingual, bias_evaluation |
| `CrowSPairs` | CrowS-Pairs | en | evaluation | CC-BY-SA-4.0 | bias_evaluation |
| `StereoSet` | StereoSet | en | evaluation | MIT | bias_evaluation |
| `RealToxicityPrompts` | RealToxicityPrompts | en | evaluation | Apache-2.0 | bias_evaluation, adversarial |
| `BoldBias` | BOLD | en | evaluation | CC-BY-4.0 | bias_evaluation |
| `DROC` | DROC | de | literature | CC-BY-4.0 | coref, literary |
| `FantasyCoref` | FantasyCoref | en | literature | Research | coref, literary |
| `BookCoref` | BOOKCOREF | en | literature | CC-BY-NC-SA-4.0 | coref, literary, long_document |
| `BookCorefSplit` | BOOKCOREF (Split) | en | literature | CC-BY-NC-SA-4.0 | coref, literary |
| `LongtoNotes` | LongtoNotes | en | mixed | CC-BY-4.0 | coref, long_document |
| `MovieCoref` | MovieCoref | en | literature | Research | coref, literary, long_document |
| `TwiConv` | TwiConv | en | social_media | Research | coref, dialogue, social_media |
| `MuDoCo` | MuDoCo | en | dialogue | MIT | coref, dialogue |
| `DialogRE` | DialogRE | en | dialogue | CC-BY-NC-SA-4.0 | dialogue, relation_extraction |
| `MultiWOZNER` | MultiWOZ NER | en | dialogue | Apache-2.0 | ner, dialogue |
| `CoQAEntities` | CoQA | en | general | Research | coref, dialogue |
| `GVC` | Gun Violence Corpus | en | news | Research | coref, event_coref |
| `FCC` | Football Coreference Corpus | en | sports | Research | coref, event_coref |
| `ECBPlusMeta` | ECB+META | en | news | Research | coref, event_coref, adversarial |
| `ARRAU` | ARRAU 3.0 (v2) | en | general | LDC + Research | coref, abstract_anaphora |
| `ISNotes` | ISNotes | en | news | Research | coref, abstract_anaphora |
| `ShellNouns` | Shell Nouns (ASN) | en | general | Research | abstract_anaphora |
| `PDTBv3` | PDTB 3.0 | en | news | LDC | abstract_anaphora |
| `CODICRACBridging` | CODI-CRAC Bridging | en | dialogue | CC-BY-4.0 | coref, dialogue, abstract_anaphora |
| `AnaphoraAccessibility` | Anaphora Accessibility | en | evaluation | Research | coref, abstract_anaphora |
| `AncientGreekUD` | Ancient Greek UD | grc | literature | CC-BY-NC-SA-3.0 | ner, ancient |
| `LatinUD` | Latin UD | la | literature | CC-BY-NC-SA-3.0 | ner, ancient |
| `CopticScriptorium` | Coptic Scriptorium | cop | religious | CC-BY-4.0 | ner, ancient |
| `LT4HALA` | LT4HALA Hebrew | hbo | religious | Research | ner, coref, ancient |
| `ORACC` | ORACC | akk | historical | CC-BY-SA-3.0 | ner, ancient |
| `MasakhaNER` | MasakhaNER | multi | news | CC-BY-4.0 | ner, multilingual, low_resource |
| `MasakhaNER2` | MasakhaNER 2.0 | multi | news | CC-BY-NC-4.0 | ner, multilingual, low_resource |
| `AfriSenti` | AfriSenti | multi | social_media | CC-BY-4.0 | multilingual, social_media, low_resource |
| `AfriQA` | AfriQA | multi | wikipedia | CC-BY-4.0 | multilingual, low_resource |
| `MasakhaNEWS` | MasakhaNEWS | multi | news | Apache-2.0 | multilingual, low_resource |
| `MasakhaPOS` | MasakhaPOS | multi | general | MIT | multilingual, low_resource |
| `WikiANN` | WikiANN | multi | wikipedia | CC-BY-SA-4.0 | ner, multilingual, low_resource |
| `NaijaNER` | NaijaNER | pcm | social_media | Research | ner, low_resource |
| `WIESP2022NER` | WIESP2022-NER (DEAL) | en | scientific | CC-BY-4.0 | ner, arcane_domain |
| `DutchArchaeology` | Dutch Archaeology NER | nl | archaeology | CC-BY-4.0 | ner, arcane_domain |
| `ENer` | E-NER (EDGAR-NER) | en | legal | GPL-3.0 | ner, arcane_domain |
| `FINER` | FINER (Food Ingredients NER) | en | food | CC-BY-4.0 | ner, arcane_domain |
| `AnnoCTR` | AnnoCTR (Cyber Threat Reports) | en | cybersecurity | CC-BY-SA-4.0 | ner, arcane_domain |
| `CRAFT` | CRAFT | en | biomedical | CC-BY-3.0 | coref, biomedical, arcane_domain |
| `WNUT16` | WNUT-16 | en | social_media | CC-BY-4.0 | ner, social_media, adversarial |
| `SanskritUD` | Sanskrit UD | sa | religious | CC-BY-SA-4.0 | ner, ancient, low_resource |
| `OldEnglishUD` | Old English UD | ang | historical | CC-BY-SA-4.0 | ner, historical, ancient, low_resource |
| `OldNorseUD` | Old Norse UD | non | literature | CC-BY-SA-4.0 | ner, literary, ancient, low_resource |
| `CALCS2018` | CALCS-2018 | multi | social_media | Research | ner, multilingual, social_media, low_resource |
| `HinglishNER` | Hinglish NER | multi | social_media | CC-BY-4.0 | ner, multilingual, social_media, low_resource |
| `MedievalCharterNER` | Medieval Charter NER | multi | historical | CC-BY-4.0 | ner, multilingual, historical, low_resource |
| `CBMACharters` | CBMA Charters | la | historical | Research | ner, historical, ancient, low_resource |
| `MSNER` | MSNER | multi | speech | CC-BY-4.0 | ner, multilingual, speech |
| `NoiseBench` | NoiseBench | en | evaluation | MIT | ner, adversarial |
| `RockNER` | RockNER | en | evaluation | Apache-2.0 | ner, adversarial |
| `CrossWeigh` | CrossWeigh | multi | evaluation | MIT | ner, multilingual, adversarial |
| `ZELDA` | ZELDA | en | wikipedia | MIT | ner, entity_linking |
| `TweetNERD` | TweetNERD | en | social_media | CC-BY-4.0 | ner, social_media |
| `AIDACoNLL` | AIDA-CoNLL | en | news | Research | ner, entity_linking |
| `ACE2005` | ACE 2005 | en | news | LDC | ner, nested_ner, relation_extraction |
| `NNE` | NNE (Nested Named Entities) | en | news | CC-BY-4.0 | ner, nested_ner |
| `GENIANested` | GENIA Nested | en | biomedical | GENIA Project License | ner, biomedical, nested_ner |
| `ChineseNestedNER` | Chinese Nested NER | zh | news | CC-BY-4.0 | ner, multilingual, nested_ner |
| `SCINERNested` | SciNER Nested | en | scientific | Apache-2.0 | ner, nested_ner, arcane_domain |
| `ShAReCLEF` | ShARe/CLEF | en | clinical | PhysioNet | ner, biomedical, discontinuous_ner |
| `GermEvalDiscontinuous` | GermEval Discontinuous | de | news | CC-BY-4.0 | ner, multilingual, discontinuous_ner |
| `ADRDiscontinuous` | ADR Discontinuous | en | biomedical | CC-BY-4.0 | ner, biomedical, discontinuous_ner, social_media |
| `PubMedDiscontinuous` | PubMed Discontinuous | en | biomedical | Research | ner, biomedical, discontinuous_ner |
| `TACRED` | TACRED | en | news | LDC | relation_extraction |
| `SemEval2010Task8` | SemEval-2010 Task 8 | en | general | Research | relation_extraction |
| `FewRel` | FewRel | en | wikipedia | MIT | relation_extraction |
| `NYT10` | NYT-10 | en | news | Research | relation_extraction |
| `JNLPBA` | JNLPBA | en | biomedical | Research | ner, biomedical |
| `S800` | S800 | en | biomedical | CC-BY-4.0 | ner, biomedical |
| `TempEval3` | TempEval-3 | en | news | CC-BY-4.0 | ner |
| `TimeBank12` | TimeBank 1.2 | en | news | LDC | ner |
| `MATRES` | MATRES | en | news | Research | ner |
| `THYME` | THYME | en | clinical | Research | ner, biomedical, clinical |
| `I2B2Temporal` | i2b2 2012 Temporal | en | clinical | Research | ner, biomedical, clinical |
| `Twitter2015MNER` | Twitter-2015 MNER | en | social_media | Research | ner, social_media |
| `DistantListeningCorpus` | Distant Listening Corpus | multi | music | CC-BY-4.0 | ner, arcane_domain |
| `PIIMasking200k` | PII Masking 200k | multi | privacy | Apache-2.0 | ner |
| `ENERSec` | E-NER SEC | en | legal | MIT | ner, arcane_domain |
| `MSNBCEL` | MSNBC | en | news | Research | entity_linking |
| `AQUAINT` | AQUAINT | en | news | LDC | entity_linking |
| `KORE50` | KORE50 | en | evaluation | CC-BY-4.0 | entity_linking, adversarial |
| `WNEDWiki` | WNED-WIKI | en | wikipedia | Research | entity_linking |
| `WNEDClueweb` | WNED-ClueWeb | en | general | Research | entity_linking |
| `BELB` | BELB | en | biomedical | Research | biomedical, entity_linking |
| `MELO` | MELO | multi | general | Apache-2.0 | multilingual, entity_linking |
| `BookCorefBamman` | BookCoref (Bamman) | en | literature | Research | coref, literary, long_document |
| `NovelCR` | NovelCR | multi | literature | Research | coref, multilingual, literary, long_document |
| `AgCNER` | AgCNER | zh | scientific | CC-BY-4.0 | ner, multilingual, long_document, arcane_domain |
| `ScrollsQMSum` | SCROLLS QMSum | en | dialogue | MIT | dialogue, long_document |
| `LongDocNER` | Long Document NER | en | general | MIT | ner, long_document |
| `BookSumCoref` | BookSum Coref | en | literature | Research | coref, literary, long_document |
| `MultiBioNERLong` | Multi-Bio Long NER | en | biomedical | Research | ner, biomedical, long_document |
| `RadCoref` | RadCoref | en | clinical | PhysioNet | coref, biomedical, clinical |
| `MEANTIME` | MEANTIME | multi | news | CC-BY-4.0 | coref, multilingual, event_coref |
| `FCCT` | FCC-T | en | sports | CC-BY-4.0 | coref, event_coref |
| `LEMONADE` | LEMONADE | multi | news | Research | coref, multilingual, event_coref |
| `BioRED` | BioRED | en | biomedical | Public | ner, biomedical, relation_extraction |
| `MedMentions` | MedMentions | en | biomedical | CC0-1.0 | ner, biomedical, entity_linking |
| `EnzChemRED` | EnzChemRED | en | biomedical | CC-BY-4.0 | ner, biomedical, relation_extraction |
| `NCERB` | NCERB | en | clinical | Research | ner, biomedical, clinical |
| `MACCROBAT` | MACCROBAT | en | biomedical | CC-BY-4.0 | ner, biomedical |
| `ACE05RE` | ACE 2005 RE | en | news | LDC | ner, relation_extraction |
| `CoNLL04RE` | CoNLL04 RE | en | news | Research | ner, relation_extraction |
| `CrossRE` | CrossRE | en | cross_domain | CC-BY-4.0 | relation_extraction |
| `UNER` | UNER | multi | general | CC-BY-SA-4.0 | ner, multilingual, low_resource |
| `IndicNER` | IndicNER | multi | general | CC-BY-4.0 | ner, multilingual, low_resource |
| `NorNE` | NorNE | no | general | CC-BY-4.0 | ner |
| `GermEval2014` | GermEval 2014 | de | news | CC-BY-4.0 | ner, nested_ner |
| `ReasoningNER` | ReasoningNER | en | evaluation | Research | ner, adversarial |
| `BioNERLLaMA` | BioNER-LLaMA | en | biomedical | Research | ner, biomedical |
| `MentionResolutionLLM` | Mention Resolution LLM | en | literature | Research | coref, literary |
| `ShARe2013` | ShARe 2013 | en | clinical | Research | ner, biomedical, discontinuous_ner, clinical |
| `ShARe2014` | ShARe 2014 | en | clinical | Research | ner, biomedical, discontinuous_ner, clinical |
| `I2B2_2010` | i2b2 2010 | en | clinical | Research | ner, biomedical, clinical |
| `LexGLUENER` | LexGLUE NER | en | legal | Research | ner, arcane_domain |
| `FinBenNER` | FinBen NER | en | financial | Research | ner, arcane_domain |
| `FiNER139` | FiNER-139 | en | financial | MIT | ner, nested_ner, arcane_domain |
| `TaggedPBCEsperanto` | taggedPBC Esperanto | eo | religious | CC-BY-4.0 | ner, low_resource, constructed |
| `TaggedPBCKlingon` | taggedPBC Klingon | tlh | religious | CC-BY-4.0 | ner, low_resource, constructed |
| `UDEsperantoCairo` | UD Esperanto Cairo | eo | general | CC-BY-SA-4.0 | ner, low_resource, constructed |
| `KlingonEffectLID` | Klingon Effect LID | multi | general | Research | multilingual, constructed, adversarial |
| `LojbanTatoeba` | Lojban Tatoeba | jbo | general | CC-BY-2.0 | low_resource, constructed |
| `InterlingueWikipedia` | Interlingue Wikipedia | ie | encyclopedia | CC-BY-SA-4.0 | low_resource, constructed |
| `TokiPonaCorpus` | Toki Pona Corpus | tok | general | CC0-1.0 | low_resource, constructed |
| `OmniNER2025` | OmniNER2025 | zh | social_media | Research | ner, multilingual, social_media |
| `LegalCore` | LegalCore | en | legal | Research | coref, event_coref, long_document, arcane_domain |
| `Zcoref` | Z-coref | multi | general | Research | coref, multilingual, abstract_anaphora |
| `MHERCL` | MHERCL | en | historical | Research | historical, entity_linking, adversarial |
| `SNOMEDChallenge` | SNOMED CT EL Challenge | en | clinical | Research | biomedical, entity_linking, clinical |
| `ESCOSkillsEL` | ESCO Skills EL | multi | general | Research | multilingual, entity_linking |
| `NatureLMAudio` | NatureLM-audio | en | bioacoustics | Research | multilingual, arcane_domain |
| `BEANSZero` | BEANS-Zero | en | bioacoustics | Research | arcane_domain, adversarial |
| `NLMChem` | NLM-Chem | en | biomedical | Public | ner, biomedical, entity_linking |
| `CHEMDNER` | CHEMDNER | en | biomedical | Research | ner, biomedical |
| `TimeBankDense` | TimeBank-Dense | en | news | Research | ner, event_coref |
| `TwitterGMNER` | Twitter-GMNER | en | social_media | CC-BY-4.0 | ner, social_media, arcane_domain |
| `MNERMI` | MNER-MI | en | social_media | CC-BY-4.0 | ner, social_media |
| `TwoMNER` | 2M-NER | multi | social_media | Apache-2.0 | ner, multilingual, social_media |
| `MathEntities` | Mathematical Entities | en | scientific | CC-BY-4.0 | ner, entity_linking, arcane_domain |
| `SciERC` | SciERC | en | scientific | CC-BY-4.0 | ner, nested_ner, relation_extraction, arcane_domain |
| `GeoWebNews` | GeoWebNews | en | news | CC-BY-4.0 | ner, entity_linking |
| `LGL` | LGL | en | news | MIT | ner, entity_linking |
| `TASTEset` | TASTEset | en | food | CC-BY-4.0 | ner, arcane_domain |
| `RecipeNER` | Recipe NER | en | food | MIT | ner, arcane_domain |
| `CodeSearchNet` | CodeSearchNet | multi | code | MIT | multilingual, arcane_domain |
| `FABLE` | FABLE | en | fiction | MIT | ner, literary |
| `ELGold` | ELGold | pl | general | CC-BY-4.0 | ner, multilingual, literary, entity_linking |
| `StreamingCDCoref` | Streaming CD-Coref | en | news | Research | coref, long_document |
| `TemDocRED` | Tem-DocRED | en | wikipedia | MIT | relation_extraction, long_document |
| `SciCoRadar` | SciCo-Radar | en | scientific | Apache-2.0 | coref, arcane_domain |
| `EventKGDrift` | Event KG Drift | en | evaluation | Research | event_coref, long_document, arcane_domain |
| `WikidataDrift` | Wikidata Semantic Drift | multi | encyclopedia | CC0-1.0 | entity_linking, adversarial |
| `AIDA` | AIDA-CoNLL (v2) | en | news | Research | entity_linking |
| `AIONER` | AIONER | en | biomedical | Research | ner, biomedical |
| `AISHELLNER` | AISHELL-NER | zh | speech | Research | ner, speech |
| `AstroNER` | AstroNER | en | astrophysics | CC-BY-4.0 | ner, arcane_domain |
| `B2NERD` | B2NERD | en | news | CC-BY-4.0 | ner |
| `BioMNER` | BioMNER | en | biomedical | Research | ner, biomedical |
| `LegNER` | LegNER | en | legal | CC-BY-4.0 | ner |
| `OpenNER` | OpenNER 1.0 | en | mixed | CC-BY-SA-4.0 | ner |
| `SciNER` | SciNER | en | scientific | Apache-2.0 | ner |
| `FinanceNER` | FinanceNER | en | financial | Research | ner |
| `TechNER` | TechNER | en | code | MIT | ner |
| `FictionNER750M` | FictionNER-750M | en | fiction | CC-BY-4.0 | ner, literary |
| `CharacterCodex` | Character Codex | en | fiction | CC-BY-4.0 | ner, literary |
| `MUC6` | MUC-6 | en | news | LDC | ner, historical |
| `MUC7` | MUC-7 | en | news | LDC | ner, historical |
| `OntoNotes50` | OntoNotes 5.0 | en | mixed | LDC | ner, coref |
| `GUM` | GUM | en | mixed | CC-BY-4.0 | ner, coref |
| `TACKBP` | TAC-KBP | en | news | LDC | entity_linking |
| `HAREM` | HAREM | pt | news | Research | ner, multilingual |
| `GunViolenceCorpus` | Gun Violence Corpus (v2) | en | news | CC-BY-4.0 | ner, event_coref |
| `SLUE` | SLUE | en | speech | MIT | ner, speech |
| `CRAFTCoref` | CRAFT Coreference | en | biomedical | CC-BY-4.0 | coref, biomedical |
| `FootballCorefCorpus` | Football Coreference Corpus (v2) | en | sports | CC-BY-4.0 | event_coref |
| `MultipartyDialogueCoref` | Multiparty Dialogue Coreference | en | dialogue | CC-BY-4.0 | coref, dialogue |
| `CODICRAC` | CODI-CRAC | multi | mixed | CC-BY-4.0 | coref, multilingual |
| `MixRED` | MixRED | en | mixed | CC-BY-4.0 | relation_extraction |
| `CovEReD` | CovEReD | en | biomedical | CC-BY-4.0 | biomedical, relation_extraction |
| `SciER` | SciER | en | scientific | Apache-2.0 | ner, nested_ner, relation_extraction |
| `WEBNLG` | WebNLG | en | wikipedia | CC-BY-4.0 | relation_extraction |
| `AkkadianUD` | Akkadian UD | akk | historical | CC-BY-SA-4.0 | historical, ancient |
| `AncientHebrewUD` | Ancient Hebrew UD | hbo | religious | CC-BY-SA-4.0 | historical, ancient |
| `ClassicalChineseUD` | Classical Chinese UD | lzh | historical | CC-BY-SA-4.0 | historical, ancient |
| `CopticUD` | Coptic UD | cop | religious | CC-BY-SA-4.0 | historical, ancient |
| `GothicUD` | Gothic UD | got | religious | CC-BY-NC-SA-4.0 | historical, ancient |
| `HittiteUD` | Hittite UD | hit | historical | CC-BY-SA-4.0 | historical, ancient |
| `OldChurchSlavonicUD` | Old Church Slavonic UD | cu | religious | CC-BY-NC-SA-4.0 | historical, ancient |
| `LatinITTB` | Latin ITTB | la | religious | CC-BY-NC-SA-3.0 | historical |
| `LatinPROIEL` | Latin PROIEL | la | historical | CC-BY-NC-SA-4.0 | historical |
| `EsperantoUD` | Esperanto UD | eo | general | CC-BY-SA-4.0 | constructed |
| `Dothraki` | Dothraki | dlk | fiction | CC-BY-SA-4.0 | constructed |
| `HighValyrian` | High Valyrian | hvy | fiction | CC-BY-SA-4.0 | constructed |
| `Klingon` | Klingon | tlh | fiction | Research | constructed |
| `Quenya` | Quenya | qya | fiction | CC-BY-4.0 | constructed |
| `Navi` | Na'vi | nav | fiction | Research | constructed |
| `Interslavic` | Interslavic | isv | general | CC-BY-SA-4.0 | constructed |
| `Lojban` | Lojban | jbo | general | Public Domain | constructed |
| `TokiPona` | Toki Pona | tok | general | CC-BY-SA-4.0 | constructed |
| `I2B22010` | i2b2-2010 | en | clinical | DUA Required | ner, relation_extraction, clinical |
| `I2b2Deidentification` | i2b2 De-identification | en | clinical | DUA Required | ner, clinical |
| `FrenchClinicalNER` | French Clinical NER | fr | clinical | DUA Required | ner, multilingual, clinical |
| `ShARe13` | ShARe/CLEF 2013 | en | clinical | PhysioNet | ner, discontinuous_ner, clinical |
| `ShARe14` | ShARe/CLEF 2014 | en | clinical | PhysioNet | ner, discontinuous_ner, clinical |
| `CALCS` | CALCS | multi | social_media | Research | ner, multilingual, social_media |
| `LinCE` | LinCE | multi | social_media | Research | ner, multilingual, social_media |
| `GLUECoS` | GLUECoS | multi | social_media | MIT | ner, multilingual, social_media |
| `ChemDataExtractor` | ChemDataExtractor | en | biomedical | MIT | ner, biomedical |
| `HUPD` | HUPD | en | legal | Public Domain | ner |
| `FinTechPatent` | FinTech Patent NER | en | financial | CC-BY-4.0 | ner |
| `WaterAgriNER` | WaterAgriNER | en | scientific | CC-BY-4.0 | ner |
| `WIESPAstro` | WIESP Astrophysics | en | astrophysics | Research | ner, arcane_domain |
| `NERsocialFood` | NER Social Food | en | food | CC-BY-4.0 | ner, social_media |
| `RussianCulturalNER` | Russian Cultural NER | ru | encyclopedia | CC-BY-4.0 | ner, multilingual |
| `EighteenthCenturyNER` | 18th Century NER | en | historical | CC-BY-4.0 | ner, historical |
| `SpanishMedievalTEI` | Spanish Medieval TEI | es | historical | CC-BY-4.0 | ner, multilingual, historical |
| `MedievalCzechCharters` | Medieval Czech Charters | cs | historical | CC-BY-4.0 | ner, multilingual, historical |
| `DutchArchaeologyNER` | Dutch Archaeology NER (v2) | nl | archaeology | CC-BY-4.0 | ner, multilingual, historical |
| `GuaraniNER` | Guaraní NER | gn | general | CC-BY-4.0 | ner, indigenous, low_resource |
| `ShipiboKoniboNER` | Shipibo-Konibo NER | shp | general | CC-BY-4.0 | ner, indigenous, low_resource |
| `NavajoMorph` | Navajo Morphology | nv | general | Research | ner, indigenous, low_resource |
| `KoCoNovel` | KoCoNovel | ko | fiction | CC-BY-SA-4.0 | coref, multilingual, literary |
| `OpenBoek` | OpenBoek | nl | fiction | CC-BY-4.0 | coref, multilingual, literary |
| `SciCo` | SciCo | en | scientific | Apache-2.0 | coref |
| `SemEval2013Task91` | SemEval-2013 Task 9.1 | en | biomedical | Research | ner, biomedical, relation_extraction |
| `PDTB3` | PDTB 3.0 (v2) | en | news | LDC | coref |
| `WinoPron` | WinoPron | en | evaluation | Research | coref |
| `QUOREF` | QUOREF | en | wikipedia | CC-BY-4.0 | coref |
| `CoNLL2002Dutch` | CoNLL-2002 Dutch | nl | news | Research | ner, multilingual |
| `CoNLL2002Spanish` | CoNLL-2002 Spanish | es | news | Research | ner, multilingual |
| `BC2GMFull` | BC2GM Full | en | biomedical | Research | ner, biomedical |
| `FinNER` | FinNER | fi | news | CC-BY-4.0 | ner, multilingual |
| `LegalNER` | LegalNER | en | legal | CC-BY-4.0 | ner |
| `CEREC` | CEREC | zh | news | CC-BY-4.0 | ner, multilingual, relation_extraction |
| `DELICATE` | DELICATE | en | clinical | Research | ner, clinical |
| `SciERCNER` | SciERC NER | en | scientific | Apache-2.0 | ner, nested_ner, relation_extraction |
| `ULNER` | ULNER | en | mixed | CC-BY-4.0 | ner |
| `UniversalNER` | UniversalNER | multi | mixed | CC-BY-4.0 | ner, multilingual |
| `ArrauGenia` | ARRAU GENIA | en | biomedical | Research | coref, biomedical |
| `ArrauPear` | ARRAU Pear Stories | en | narrative | Research | coref, literary |
| `ArrauRst` | ARRAU RST | en | news | Research | coref |
| `ArrauTrains` | ARRAU Trains | en | dialogue | Research | coref, dialogue |
| `NomBankImplicit` | NomBank Implicit | en | news | LDC | coref |
| `BASHI` | BASHI | bn | news | Research | ner, multilingual, low_resource |
| `ERST` | ERST | en | mixed | CC-BY-4.0 | coref |
| `BiTimeBERT` | BiTimeBERT | en | news | CC-BY-4.0 | ner |
| `TRIDIS` | TRIDIS | en | mixed | CC-BY-4.0 | coref |
| `QueerBench` | QueerBench | en | evaluation | CC-BY-4.0 | coref, bias_evaluation |
| `QUEEREOTYPES` | QUEEREOTYPES | en | evaluation | CC-BY-4.0 | bias_evaluation |
| `MAP` | MAP | en | clinical | DUA Required | ner, clinical |
| `ASN` | ASN | en | news | Research | relation_extraction |
| `CSN` | CSN | multi | code | MIT | ner |
| `HOMOMEX` | HOMOMEX | es | general | CC-BY-4.0 | multilingual |
| `ENER` | ENER | en | general | CC-BY-4.0 | ner |
| `FIREBALL` | FIREBALL | en | gaming | CC-BY-4.0 | ner, dialogue |
| `DnDNERBenchmark` | D&D NER Benchmark | en | gaming | Research | ner, literary |
| `CriticalRoleDataset` | Critical Role Dataset | en | gaming | Research | ner, literary, dialogue |
| `CUAD` | CUAD | en | legal | CC-BY-4.0 | ner |
| `ACORD` | ACORD | en | legal | Research | ner |
| `PartyExtractionDataset` | Party Extraction Dataset | en | legal | Research | ner |
| `FINERFood` | FINER (Food) | en | food | CC-BY-4.0 | ner, arcane_domain |
| `NHKRecipeDataset` | NHK Recipe Dataset | ja | food | Research | ner, multilingual, arcane_domain |
| `SanskritNERBhagavadGita` | Sanskrit NER (Bhagavad Gita) | sa | religious | Research | ner, ancient, arcane_domain |
| `AkkadianCuneiformDataset` | Akkadian Cuneiform Dataset | akk | historical | CC-BY-4.0 | ner, historical, ancient |
| `HeidelbergCuneiformBenchmark` | Heidelberg Cuneiform Benchmark | akk | historical | Research | ner, historical, ancient |
| `GreekMythologyKG` | Greek Mythology Knowledge Graph | en | mythology | CC-BY-4.0 | ner, coref, arcane_domain |
| `FolkloreMotifDistribution` | Folklore Motif Distribution | multi | mythology | Research | ner, multilingual, arcane_domain |
| `NDNER` | ND-NER | en | defense | CC-BY-SA-4.0 | ner, nested_ner, arcane_domain |
| `Re3dDefense` | re3d (Defense) | en | defense | OGL | ner, relation_extraction, arcane_domain |
| `CyNERAptner` | CyNER-APTNER | en | cybersecurity | Research | ner, arcane_domain |
| `ChineseEngineeringGeologyNER` | Chinese Engineering Geology NER | zh | geology | Research | ner, multilingual, arcane_domain |
| `LLMRocMinNER` | LLM-RocMin-NER | en | geology | CC-BY-4.0 | ner, nested_ner, arcane_domain |
| `PolyIE` | PolyIE | en | materials | CC-BY-4.0 | ner, relation_extraction, arcane_domain |
| `MathDial` | MathDial | en | education | CC-BY-4.0 | ner, dialogue, arcane_domain |
| `CoMTA` | CoMTA | en | education | Research | ner, dialogue, arcane_domain |
| `FrenchFullLengthFictionCoref` | French Full-Length Fiction Coreference | fr | fiction | CC-BY-4.0 | coref, multilingual, literary, long_document |
| `WinogradSchemaChallengeWSC` | Winograd Schema Challenge | en | evaluation | Research | coref, bias_evaluation |
| `TVShowMultilingualCoref` | TV Show Multilingual Coreference | multi | dialogue | Research | coref, multilingual, dialogue |
| `VisDialCoref` | VisDial Coreference | en | vision | CC-BY-4.0 | coref, dialogue |
| `RISeC` | RISeC | en | food | CC-BY-4.0 | coref, arcane_domain |
| `EFGC` | EFGC | en | food | CC-BY-4.0 | coref, arcane_domain |
| `SPoRC` | SPoRC | en | speech | Research | ner, dialogue, speech |
| `ARFFiction` | ARF (Artificial Relationships in Fiction) | en | fiction | CC-BY-4.0 | literary, relation_extraction |
| `CRAFTCorpusCoref` | CRAFT Corpus (Full Coref) | en | biomedical | CC-BY-4.0 | coref, biomedical, long_document |
| `AerospaceNERDataset` | Aerospace NER Dataset | en | aerospace | Research | ner, arcane_domain |
| `AviationProductsNER` | Aviation Products NER | zh | aerospace | Research | ner, multilingual, arcane_domain |
| `VREN` | VREN (Volleyball) | en | sports | CC-BY-4.0 | ner, arcane_domain |
| `FashionIQ` | Fashion IQ | en | fashion | Research | ner, arcane_domain |
| `NaturalProductsRE` | Natural Products RE | en | biomedical | Research | biomedical, relation_extraction |
| `DrugProtBioCreative` | DrugProt | en | biomedical | Research | biomedical, relation_extraction |
| `MOFDataset` | MOF Dataset | en | materials | CC-BY-4.0 | ner, relation_extraction, arcane_domain |
| `SolidStateDoping` | Solid-State Doping | en | materials | CC-BY-4.0 | ner, relation_extraction, arcane_domain |
| `AgriNER` | AgriNER | en | agriculture | Research | ner, relation_extraction, arcane_domain |
| `AGRONER` | AGRONER | en | agriculture | Research | ner, arcane_domain |
| `AgMNER` | AgMNER | zh | agriculture | CC-BY-4.0 | ner, multilingual, arcane_domain, speech |
| `PolishCoreferenceCorpus` | Polish Coreference Corpus | pl | general | CC-BY-SA-4.0 | coref, multilingual |
| `ArabicEventCoref` | Arabic Event Coreference | ar | news | Research | coref, multilingual, event_coref |
| `HindiEnglishSocialMediaNER` | Hindi-English Social Media NER | hi-en | social_media | Research | ner, multilingual, social_media, low_resource |
| `AstroBERTCorpus` | astroBERT Corpus | en | astronomy | Research | ner, arcane_domain |
| `AstronomicalTelegramKEE` | Astronomical Telegram KEE | en | astronomy | Research | ner, arcane_domain |
| `Saraga` | Saraga | multi | music | CC-BY-4.0 | ner, multilingual, arcane_domain |
| `MusicBrainzRE` | MusicBrainz RE | en | music | CC0 | relation_extraction, arcane_domain |
| `DINAA` | DINAA | en | archaeology | CC-BY-4.0 | ner, arcane_domain |
| `IMDbSemiStructuredRE` | IMDb Semi-Structured RE | en | entertainment | Research | relation_extraction, arcane_domain |
| `ATISFlightBooking` | ATIS Flight Booking | en | travel | Research | ner |
| `PaleontologyNER` | Paleontology NER | en | paleontology | Research | ner, arcane_domain |
| `WaterResourceNER` | Water Resource NER | en | environment | CC-BY-4.0 | ner, arcane_domain |
| `MalwareTextDB` | MalwareTextDB | en | cybersecurity | Research | ner, arcane_domain |
| `SECFilingsNER` | SEC-filings | en | finance | CC-BY-3.0 | ner |
| `AnEM` | AnEM | en | biomedical | CC-BY-SA-3.0 | ner, biomedical |
| `RecipeDBAnnotated` | RecipeDB Annotated | en | food | CC-BY-4.0 | ner, arcane_domain |
| `RitterTwitterNER` | Ritter Twitter NER | en | social_media | Research | ner, social_media |
| `MusicNER` | Music-NER | en | music | MIT | ner, arcane_domain |
| `TutoringSessionsAlgebra` | 500 Tutoring Sessions | en | education | Research | ner, dialogue, arcane_domain |
| `GNERGeoscience` | GNER | zh | geology | Research | ner, multilingual, arcane_domain |
| `FourRegionsGeologyNER` | Four Regions Geology NER | zh | geology | Research | ner, multilingual, arcane_domain |
| `MSPPodcast` | MSP-Podcast | en | speech | Research | ner, arcane_domain, speech |
| `SpotifyPodcastsDataset` | Spotify Podcasts Dataset | en | speech | Research | ner, arcane_domain, speech |
| `SportsNERGeneral` | Sports NER | en | sports | Research | ner, arcane_domain |
| `EsportsNER` | Esports NER | en | gaming | Research | ner, arcane_domain |
| `DeepFashion2` | DeepFashion2 | en | fashion | Research | ner, arcane_domain |
| `ConstructionNER` | Construction NER | en | construction | Research | ner, arcane_domain |
| `PharmaNER` | PharmaNER | en | biomedical | Research | ner, biomedical, clinical |
| `ProductReviewNER` | Product Review NER | en | ecommerce | CC-BY-4.0 | ner |
| `RealEstateNER` | Real Estate NER | en | real_estate | Research | ner, arcane_domain |
| `AutomotiveNER` | Automotive NER | en | automotive | Research | ner, arcane_domain |
| `TourismNER` | Tourism NER | en | tourism | CC-BY-4.0 | ner, arcane_domain |
| `EnergyNER` | Energy NER | en | energy | Research | ner, arcane_domain |
| `InsuranceNER` | Insurance NER | en | insurance | Research | ner, arcane_domain |
| `LogisticsNER` | Logistics NER | en | logistics | Research | ner, arcane_domain |
| `ResumeNER` | Resume NER | en | hr | CC0 | ner |
| `JobPostingNER` | Job Posting NER | en | hr | Research | ner, arcane_domain |
| `HealthcareAdminNER` | Healthcare Admin NER | en | healthcare | Research | ner, clinical, arcane_domain |
| `TelecomNER` | Telecom NER | en | telecom | Research | ner, arcane_domain |
| `WeatherNER` | Weather NER | en | weather | CC-BY-4.0 | ner, arcane_domain |
| `ManufacturingNER` | Manufacturing NER | en | manufacturing | Research | ner, arcane_domain |
| `RetailInventoryNER` | Retail Inventory NER | en | retail | Research | ner, arcane_domain |
| `CropDiseaseNER` | Crop Disease NER | en | agriculture | CC-BY-4.0 | ner, arcane_domain |
| `WineNER` | Wine NER | en | food | CC-BY-4.0 | ner, arcane_domain |
| `VeterinaryNER` | Veterinary NER | en | veterinary | Research | ner, arcane_domain |
| `PhotographyNER` | Photography NER | en | photography | CC-BY-4.0 | ner, arcane_domain |
| `GenealogyNER` | Genealogy NER | en | genealogy | CC-BY-4.0 | ner, historical, arcane_domain |
| `BoardGameNER` | Board Game NER | en | gaming | CC-BY-4.0 | ner, arcane_domain |
| `GardeningNER` | Gardening NER | en | gardening | CC-BY-4.0 | ner, arcane_domain |
| `BrewingNER` | Brewing NER | en | food | CC-BY-4.0 | ner, arcane_domain |
| `KnittingNER` | Knitting NER | en | crafts | CC-BY-4.0 | ner, arcane_domain |
| `FitnessNER` | Fitness NER | en | fitness | CC-BY-4.0 | ner, arcane_domain |
| `AstrologyNER` | Astrology NER | en | astrology | CC-BY-4.0 | ner, arcane_domain |
| `TattooNER` | Tattoo NER | en | art | CC-BY-4.0 | ner, arcane_domain |
| `FragranceNER` | Fragrance NER | en | fragrance | CC-BY-4.0 | ner, arcane_domain |
| `ChessNER` | Chess NER | en | gaming | CC-BY-4.0 | ner, arcane_domain |
| `CocktailNER` | Cocktail NER | en | food | CC-BY-4.0 | ner, arcane_domain |
| `AntiquesNER` | Antiques NER | en | antiques | CC-BY-4.0 | ner, historical, arcane_domain |
| `MaritimeNER` | Maritime NER | en | maritime | Research | ner, arcane_domain |
| `EquestrianNER` | Equestrian NER | en | equestrian | CC-BY-4.0 | ner, arcane_domain |
| `WoodworkingNER` | Woodworking NER | en | crafts | CC-BY-4.0 | ner, arcane_domain |
| `BirdwatchingNER` | Birdwatching NER | en | wildlife | CC-BY-4.0 | ner, arcane_domain |
| `NumismaticsNER` | Numismatics NER | en | numismatics | CC-BY-4.0 | ner, arcane_domain |
| `PhilatelyNER` | Philately NER | en | philately | CC-BY-4.0 | ner, arcane_domain |
| `ScubaNER` | Scuba NER | en | scuba | CC-BY-4.0 | ner, arcane_domain |
| `ThemeParkNER` | Theme Park NER | en | entertainment | CC-BY-4.0 | ner, arcane_domain |
| `OrigamiNER` | Origami NER | en | crafts | CC-BY-4.0 | ner, arcane_domain |
| `AnimeMangaNER` | Anime/Manga NER | multi | entertainment | CC-BY-4.0 | ner, multilingual, arcane_domain |
| `CryptoNER` | Crypto NER | en | crypto | Research | ner, arcane_domain |
| `TelenovelaNER` | Telenovela NER | es | entertainment | CC-BY-4.0 | ner, multilingual, arcane_domain |
| `TarotNER` | Tarot NER | en | divination | CC-BY-4.0 | ner, arcane_domain |
| `BeekeepingNER` | Beekeeping NER | en | agriculture | CC-BY-4.0 | ner, arcane_domain |

## Dataset Details

### WikiGold

**Rust ID**: `DatasetId::WikiGold`

Wikipedia-based NER (PER, LOC, ORG, MISC). Historically significant as early Wikipedia NER resource.

- **Language**: en
- **Domain**: wikipedia
- **Entity Types**: PER, LOC, ORG, MISC
- **Year**: 2009
- **Format**: CoNLL
- **Annotation Scheme**: IOB2
- **Size**: ~40k tokens, ~3,500 entities
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Balasuriya et al. (2009)
- **Paper**: <https://aclanthology.org/U09-1001/>
- **URL**: <https://raw.githubusercontent.com/juand-r/entity-recognition-datasets/master/data/wikigold/CONLL-format/data/wikigold.conll.txt>

**Example**:
```
Japan B-LOC
's O
Minister O
Shinzo B-PER
Abe I-PER
visited O
the O
United B-LOC
States I-LOC
. O
```

### WNUT-17

**Rust ID**: `DatasetId::Wnut17`

Social media NER with emerging entities. Created to evaluate models on rare/emerging entities in noisy social text.

- **Language**: en
- **Domain**: social_media
- **Entity Types**: person, location, corporation, product, creative-work, group
- **Year**: 2017
- **Format**: CoNLL
- **Annotation Scheme**: BIO
- **Size**: ~65k tokens, 1,000 tweets
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Derczynski et al. (2017)
- **Paper**: <https://aclanthology.org/W17-4418/>
- **Notes**: 89% unseen entities in test set - excellent for OOD evaluation; shared task at W-NUT workshop
- **URL**: <https://raw.githubusercontent.com/leondz/emerging_entities_17/master/emerging.test.annotated>

### MIT Movie

**Rust ID**: `DatasetId::MitMovie`

Movie domain slot filling NER. Created at MIT SLS for spoken language understanding research.

- **Language**: en
- **Domain**: entertainment
- **Entity Types**: Actor, Director, Genre, Title, Year, Song, Character, Plot, Rating
- **Year**: 2013
- **Format**: BIO
- **Annotation Scheme**: BIO
- **Size**: ~12k utterances
- **License**: Research
- **Citation**: Liu et al. (2013)
- **Paper**: <https://groups.csail.mit.edu/sls/publications/2013/Liu_ASRU_2013.pdf>
- **URL**: <https://groups.csail.mit.edu/sls/downloads/movie/engtest.bio>

**Example**:
```
show O
me O
action B-Genre
movies O
directed O
by O
steven B-Director
spielberg I-Director
```

### MIT Restaurant

**Rust ID**: `DatasetId::MitRestaurant`

Restaurant domain slot filling NER. Part of MIT SLS spoken dialogue systems research.

- **Language**: en
- **Domain**: restaurant
- **Entity Types**: Amenity, Cuisine, Dish, Hours, Location, Price, Rating, Restaurant_Name
- **Year**: 2013
- **Format**: BIO
- **Annotation Scheme**: BIO
- **Size**: ~8k utterances
- **License**: Research
- **Citation**: Liu et al. (2013)
- **Paper**: <https://groups.csail.mit.edu/sls/publications/2013/Liu_ASRU_2013.pdf>
- **URL**: <https://groups.csail.mit.edu/sls/downloads/restaurant/restauranttest.bio>

**Example**:
```
find O
italian B-Cuisine
restaurants O
in O
boston B-Location
with O
outdoor B-Amenity
seating I-Amenity
```

### CoNLL-2003 Sample

**Rust ID**: `DatasetId::CoNLL2003Sample`

Classic news NER benchmark from Reuters Corpus. Foundational dataset that established modern NER evaluation standards.

- **Language**: en
- **Domain**: news
- **Entity Types**: PER, LOC, ORG, MISC
- **Year**: 2003
- **Format**: CoNLL
- **Annotation Scheme**: IOB2
- **Size**: ~300k tokens, ~35k entities
- **License**: Research
- **Citation**: Tjong Kim Sang & De Meulder (2003)
- **Paper**: <https://aclanthology.org/W03-0419/>
- **Notes**: Original has ~7% annotation errors (CleanCoNLL 2023); still the most-cited NER benchmark
- **URL**: <https://raw.githubusercontent.com/autoih/conll2003/master/CoNLL-2003/eng.testb>

**Example**:
```
EU B-ORG
rejects O
German B-MISC
call O
to O
boycott O
British B-MISC
lamb O
. O
```

### OntoNotes Sample

**Rust ID**: `DatasetId::OntoNotesSample`

Multi-genre 18-type NER from OntoNotes 5.0. Rich annotation including coreference, parsing, and PropBank.

- **Language**: en
- **Domain**: news
- **Entity Types**: PERSON, ORG, GPE, LOC, DATE, TIME, MONEY, PERCENT, NORP, FAC, PRODUCT, EVENT, WORK_OF_ART, LAW, LANGUAGE, QUANTITY, ORDINAL, CARDINAL
- **Year**: 2013
- **Format**: CoNLL
- **Annotation Scheme**: IOB2
- **Size**: ~1.6M tokens, ~128k entities
- **License**: LDC
- **Citation**: Weischedel et al. (2013)
- **Paper**: <https://catalog.ldc.upenn.edu/LDC2013T19>
- **Notes**: Full corpus requires LDC license; sample for testing; includes 7 genres
- **URL**: <https://raw.githubusercontent.com/autoih/conll2003/master/CoNLL-2003/eng.testb>

**Example**:
```
The B-ORG
European I-ORG
Union I-ORG
announced O
Monday B-DATE
that O
the O
$ B-MONEY
10 I-MONEY
million I-MONEY
will O
go O
to O
Ukraine B-GPE
. O
```

### MultiNERD

**Rust ID**: `DatasetId::MultiNERD`

Large multilingual NER covering 10 languages. Created to address scarcity of multilingual fine-grained NER data.

- **Language**: en
- **Domain**: wikipedia
- **Entity Types**: PER, LOC, ORG, ANIM, BIO, CEL, DIS, EVE, FOOD, INST, MEDIA, MYTH, PLANT, TIME, VEHI
- **Year**: 2022
- **Format**: JSONL
- **Annotation Scheme**: BIO
- **Size**: ~1M sentences across 10 languages
- **License**: CC-BY-SA-4.0 (SPDX)
- **Citation**: Tedeschi & Navigli (2022)
- **Paper**: <https://aclanthology.org/2022.findings-naacl.60/>
- **URL**: <https://huggingface.co/datasets/Babelscape/multinerd/resolve/main/test/test_en.jsonl>

**Example**:
```
Marie Curie (PER) discovered radium at the University of Paris (ORG) in France (LOC).
```

### Few-NERD

**Rust ID**: `DatasetId::FewNERD`

Fine-grained NER with 66 types in 8 coarse categories. Designed for few-shot learning evaluation.

- **Language**: en
- **Domain**: wikipedia
- **Entity Types**: person, location, organization, building, art, product, event, other
- **Year**: 2021
- **Format**: TSV
- **Size**: 188k sentences, 66 fine-grained types
- **License**: CC-BY-SA-4.0 (SPDX)
- **Citation**: Ding et al. (2021)
- **Paper**: <https://aclanthology.org/2021.acl-long.248/>
- **Notes**: Hierarchical type system; benchmark for few-shot and fine-grained NER
- **URL**: <https://huggingface.co/datasets/DFKI-SLT/few-nerd>

**Example**:
```
Elon Musk (person-entrepreneur) founded SpaceX (organization-company) in Hawthorne (location-city), California.
```

### CrossNER

**Rust ID**: `DatasetId::CrossNER`

Cross-domain NER across 5 domains: politics, science, music, literature, AI. Tests domain transfer.

- **Language**: en
- **Domain**: multi-domain
- **Entity Types**: PER, ORG, LOC, MISC, Domain-specific
- **Year**: 2021
- **Format**: CoNLL
- **Size**: 5 domains, ~10k sentences each
- **License**: MIT (SPDX)
- **Citation**: Liu et al. (2021)
- **Paper**: <https://aclanthology.org/2021.aaai.main.672/>
- **Notes**: Tests cross-domain transfer; domain-specific entity types. Use HuggingFace datasets library to load.
- **URL**: <https://huggingface.co/datasets/DFKI-SLT/cross_ner>

### FabNER

**Rust ID**: `DatasetId::FabNER`

Manufacturing domain NER. 12 entity types for Industry 4.0 applications.

- **Language**: en
- **Domain**: manufacturing
- **Entity Types**: Material, Process, Machine, Product, Property
- **Year**: 2022
- **Format**: CoNLL
- **Size**: ~14k sentences, 12 entity types
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Kumar et al. (2022)
- **Paper**: <https://aclanthology.org/2022.lrec-1.227/>
- **Notes**: Specialized manufacturing/engineering domain; Industry 4.0
- **URL**: <https://huggingface.co/datasets/DFKI-SLT/fabner>

### Broad Twitter Corpus

**Rust ID**: `DatasetId::BroadTwitterCorpus`

Twitter NER across multiple time periods. Tests temporal robustness of NER systems.

- **Language**: en
- **Domain**: social_media
- **Entity Types**: PER, LOC, ORG, MISC
- **Year**: 2016
- **Format**: BIO
- **Size**: ~9k tweets, stratified by time period
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Derczynski et al. (2016)
- **Paper**: <https://aclanthology.org/C16-1111/>
- **Notes**: Temporal stratification; tests model robustness to language evolution
- **URL**: <https://raw.githubusercontent.com/GateNLP/broad_twitter_corpus/master/test.bio>

### WikiNeural

**Rust ID**: `DatasetId::WikiNeural`

Silver-standard multilingual NER from Wikipedia. 9 languages with automatic annotation.

- **Language**: multi
- **Domain**: wikipedia
- **Entity Types**: PER, LOC, ORG, MISC
- **Year**: 2021
- **Format**: CoNLL
- **Size**: 9 languages, ~100k sentences each
- **License**: CC-BY-SA-4.0 (SPDX)
- **Citation**: Tedeschi et al. (2021)
- **Paper**: <https://aclanthology.org/2021.findings-emnlp.215/>
- **Notes**: Automatically generated silver annotations; useful for pre-training
- **URL**: <https://huggingface.co/datasets/Babelscape/wikineural>

### Polyglot-NER

**Rust ID**: `DatasetId::PolyglotNER`

Massively multilingual NER. 40 languages with silver annotations from Wikipedia.

- **Language**: multi
- **Domain**: wikipedia
- **Entity Types**: PER, LOC, ORG
- **Year**: 2015
- **Format**: CoNLL
- **Size**: 40 languages, silver annotations
- **License**: Research
- **Citation**: Al-Rfou et al. (2015)
- **Paper**: <https://aclanthology.org/C14-1078/>
- **Notes**: Largest language coverage; silver annotations via Wikipedia links
- **URL**: <https://sites.google.com/site/rmaborhoo/polyglot-ner>

### Universal NER

**Rust ID**: `DatasetId::UniversalNERBench`

Cross-lingual NER benchmark spanning 13 diverse languages. Tests zero-shot transfer.

- **Language**: multi
- **Domain**: mixed
- **Entity Types**: PER, LOC, ORG
- **Year**: 2022
- **Format**: CoNLL
- **Size**: 13 languages, gold annotations
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Malmasi et al. (2022)
- **Paper**: <https://aclanthology.org/2022.emnlp-main.13/>
- **Notes**: Tests cross-lingual zero-shot transfer; diverse language families
- **URL**: <https://github.com/UniversalNER/UNER>

### CoNLL-2002

**Rust ID**: `DatasetId::CoNLL2002`

Spanish and Dutch NER from CoNLL 2002 shared task. Multi-language NER benchmark.

- **Language**: multi
- **Domain**: news
- **Entity Types**: PER, LOC, ORG, MISC
- **Year**: 2002
- **Format**: CoNLL
- **Annotation Scheme**: BIO
- **Size**: Spanish + Dutch news articles
- **License**: Research
- **Citation**: Tjong Kim Sang (2002)
- **Paper**: <https://aclanthology.org/W02-2024/>
- **Notes**: First multilingual NER shared task; established CoNLL NER format
- **URL**: <https://www.clips.uantwerpen.be/conll2002/ner/>

### TweetNER7

**Rust ID**: `DatasetId::TweetNER7`

Twitter NER across 7 entity types. Fine-grained social media NER with temporal annotations.

- **Language**: en
- **Domain**: social_media
- **Entity Types**: person, location, corporation, product, creative_work, group, event
- **Year**: 2022
- **Format**: JSONL
- **Size**: ~12k tweets
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Ushio et al. (2022)
- **Paper**: <https://aclanthology.org/2022.findings-emnlp.304/>
- **Notes**: Temporal distribution shift; tests robustness to evolving language
- **URL**: <https://huggingface.co/datasets/tner/tweetner7>

### Google-RE

**Rust ID**: `DatasetId::GoogleRE`

Google Relation Extraction dataset. Wikipedia sentences with relation annotations.

- **Language**: en
- **Domain**: wikipedia
- **Entity Types**: PER, LOC, ORG
- **Year**: 2017
- **Format**: JSONL
- **Size**: ~60k relation triples
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Levy et al. (2017)
- **Paper**: <https://aclanthology.org/D17-1004/>
- **Notes**: Clean relation extraction; commonly used for zero-shot RE evaluation
- **URL**: <https://github.com/google-research-datasets/relation-extraction-corpus>

### NYT-FB

**Rust ID**: `DatasetId::NYTFB`

New York Times with Freebase relations. Distant supervision relation extraction.

- **Language**: en
- **Domain**: news
- **Entity Types**: PER, LOC, ORG
- **Year**: 2010
- **Format**: JSONL
- **Size**: ~570k sentences, 53 relations
- **License**: Research
- **Citation**: Riedel et al. (2010)
- **Paper**: <https://aclanthology.org/N10-1114/>
- **Notes**: Classic distant supervision RE; noisy but large-scale
- **URL**: <https://github.com/thunlp/OpenNRE>

### REBEL

**Rust ID**: `DatasetId::REBEL`

Relation Extraction By End-to-end Language generation. Large-scale RE dataset.

- **Language**: en
- **Domain**: wikipedia
- **Entity Types**: PER, LOC, ORG, Event
- **Year**: 2021
- **Format**: JSONL
- **Size**: ~6M triples from Wikipedia
- **License**: CC-BY-SA-4.0 (SPDX)
- **Citation**: Huguet Cabot & Navigli (2021)
- **Paper**: <https://aclanthology.org/2021.findings-emnlp.204/>
- **Notes**: Large-scale; generative RE approach; 220 relation types
- **URL**: <https://huggingface.co/datasets/Babelscape/rebel-dataset>

### MultiCoNER

**Rust ID**: `DatasetId::MultiCoNER`

Multilingual Complex NER. 11 languages with fine-grained and complex entities.

- **Language**: multi
- **Domain**: mixed
- **Entity Types**: PER, LOC, CORP, GRP, PROD, CW
- **Year**: 2022
- **Format**: CoNLL
- **Size**: 11 languages, ~1.1M tokens
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Malmasi et al. (2022)
- **Paper**: <https://aclanthology.org/2022.semeval-1.196/>
- **Notes**: SemEval-2022 shared task; complex entities from diverse sources
- **URL**: <https://multiconer.github.io/>

### MultiCoNER v2

**Rust ID**: `DatasetId::MultiCoNERv2`

MultiCoNER v2 with expanded languages and fine-grained types.

- **Language**: multi
- **Domain**: mixed
- **Entity Types**: PER, LOC, CORP, GRP, PROD, CW, Medical, Scientist
- **Year**: 2023
- **Format**: CoNLL
- **Size**: 12 languages, fine-grained types
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Fetahu et al. (2023)
- **Paper**: <https://aclanthology.org/2023.semeval-1.43/>
- **Notes**: SemEval-2023 shared task; expanded from v1 with more types
- **URL**: <https://multiconer.github.io/>

### BC5CDR

**Rust ID**: `DatasetId::BC5CDR`

Biomedical NER for diseases and chemicals. Created for BioCreative V CDR task, a major biomedical NLP benchmark.

- **Language**: en
- **Domain**: biomedical
- **Entity Types**: Chemical, Disease
- **Year**: 2016
- **Format**: BIO
- **Annotation Scheme**: BIO
- **Size**: ~1500 PubMed abstracts, ~14k mentions
- **License**: Public (SPDX)
- **Citation**: Li et al. (2016)
- **Paper**: <https://academic.oup.com/database/article/doi/10.1093/database/baw068/2630414>
- **URL**: <https://raw.githubusercontent.com/shreyashub/BioFLAIR/master/data/ner/bc5cdr/test.txt>

**Example**:
```
Aspirin B-Chemical
induced O
hepatotoxicity B-Disease
was O
observed O
. O
```

### NCBI Disease

**Rust ID**: `DatasetId::NCBIDisease`

NCBI disease mentions corpus. Foundational resource for disease NER from NIH.

- **Language**: en
- **Domain**: biomedical
- **Entity Types**: Disease
- **Year**: 2014
- **Format**: BIO
- **Annotation Scheme**: BIO
- **Size**: ~800 PubMed abstracts, ~6k mentions
- **License**: Public (SPDX)
- **Citation**: Dogan et al. (2014)
- **Paper**: <https://www.sciencedirect.com/science/article/pii/S1532046413001974>
- **URL**: <https://raw.githubusercontent.com/shreyashub/BioFLAIR/master/data/ner/NCBI-disease/test.txt>

**Example**:
```
The O
patient O
was O
diagnosed O
with O
type B-Disease
2 I-Disease
diabetes I-Disease
. O
```

### GENIA

**Rust ID**: `DatasetId::GENIA`

Biomedical NER for molecular biology. First large-scale biomedical NER corpus; historically significant.

- **Language**: en
- **Domain**: biomedical
- **Entity Types**: DNA, RNA, protein, cell_line, cell_type
- **Year**: 2003
- **Format**: XML
- **Annotation Scheme**: Standoff
- **Size**: 2000 MEDLINE abstracts, ~100k entities
- **License**: GENIA Project License
- **Citation**: Kim et al. (2003)
- **Paper**: <https://academic.oup.com/bioinformatics/article/19/suppl_1/i180/227927>
- **Notes**: Nested entities common; requires special handling; pioneered biomedical NER
- **URL**: <https://huggingface.co/datasets/chufangao/GENIA-NER>

### AnatEM

**Rust ID**: `DatasetId::AnatEM`

Anatomical entity mention corpus. 1,212 PubMed abstracts with anatomical structures.

- **Language**: en
- **Domain**: biomedical
- **Entity Types**: Anatomy
- **Year**: 2012
- **Format**: Standoff
- **Size**: 1,212 abstracts, ~7k entity mentions
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Ohta et al. (2012)
- **Paper**: <https://aclanthology.org/W12-2402/>
- **Notes**: Fine-grained anatomical mentions; standalone or nested within other entities
- **URL**: <https://huggingface.co/datasets/disi-unibo-nlp/AnatEM>

### BC2GM

**Rust ID**: `DatasetId::BC2GM`

BioCreative II Gene Mention recognition. Gold-standard gene/protein name tagging.

- **Language**: en
- **Domain**: biomedical
- **Entity Types**: Gene, Protein
- **Year**: 2008
- **Format**: IOB2
- **Size**: 20k sentences, ~24k gene mentions
- **License**: Research
- **Citation**: Smith et al. (2008)
- **Paper**: <https://genomebiology.biomedcentral.com/articles/10.1186/gb-2008-9-s2-s2>
- **Notes**: Classic benchmark for gene/protein NER; BioCreative shared task
- **URL**: <https://huggingface.co/datasets/bigbio/bc2gm_corpus>

### BC4CHEMD

**Rust ID**: `DatasetId::BC4CHEMD`

BioCreative IV Chemical Entity Mention Detection. Drug and chemical name recognition.

- **Language**: en
- **Domain**: biomedical
- **Entity Types**: Chemical
- **Year**: 2015
- **Format**: IOB2
- **Size**: 10k PubMed abstracts, ~84k chemical mentions
- **License**: Research
- **Citation**: Krallinger et al. (2015)
- **Paper**: <https://jcheminf.biomedcentral.com/articles/10.1186/1758-2946-7-S1-S2>
- **Notes**: Chemical NER benchmark; includes IUPAC names, trivial names, abbreviations
- **URL**: <https://huggingface.co/datasets/bigbio/bc4chemd>

### GAP

**Rust ID**: `DatasetId::GAP`

Gender Ambiguous Pronoun resolution. Google's benchmark for exposing gender bias in coreference systems.

- **Language**: en
- **Domain**: wikipedia
- **Entity Types**: PER
- **Year**: 2018
- **Format**: TSV
- **Size**: 8,908 pronoun-name pairs
- **License**: Apache-2.0 (SPDX)
- **Citation**: Webster et al. (2018)
- **Paper**: <https://aclanthology.org/Q18-1042/>
- **Notes**: Designed to expose gender bias; Kaggle shared task; balanced male/female
- **URL**: <https://raw.githubusercontent.com/google-research-datasets/gap-coreference/master/gap-test.tsv>

**Example**:
```
ID	Text	Pronoun	A	B	A-coref
test-1	Zoe met Alice and she waved.	she	Zoe	Alice	FALSE
```

### PreCo

**Rust ID**: `DatasetId::PreCo`

Large-scale coreference from PreCo reading comprehension corpus. 10x larger than OntoNotes.

- **Language**: en
- **Domain**: general
- **Entity Types**: PER, LOC, ORG
- **Year**: 2018
- **Format**: JSONL
- **Annotation Scheme**: CoNLLCoref
- **Size**: 38k documents, includes singletons
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Chen et al. (2018)
- **Paper**: <https://aclanthology.org/D18-1016/>
- **Notes**: Preschool vocabulary for cleaner evaluation; largest public coref corpus
- **URL**: <https://preschool-lab.github.io/PreCo/>

### LitBank

**Rust ID**: `DatasetId::LitBank`

Literary coreference. 100 public-domain English fiction works (1719-1922) with ACE-style entities.

- **Language**: en
- **Domain**: literature
- **Entity Types**: PER, LOC, ORG, GPE, FAC, VEH
- **Year**: 2019
- **Format**: BRAT
- **Annotation Scheme**: Standoff
- **Size**: 100 novels, ~2k tokens each
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Bamman et al. (2019)
- **Paper**: <https://aclanthology.org/P19-1353/>
- **Notes**: Focus on character coreference; includes event coref; public domain texts
- **URL**: <https://raw.githubusercontent.com/dbamman/litbank/master/coref/brat/1023_bleak_house_brat.ann>

### ECB+

**Rust ID**: `DatasetId::ECBPlus`

Event Coreference Bank Plus. Standard benchmark for cross-document event coreference resolution.

- **Language**: en
- **Domain**: news
- **Entity Types**: EVENT, TIME, LOC, PARTICIPANT
- **Year**: 2014
- **Format**: Custom
- **Size**: 43 topics, 982 docs, ~7k events
- **License**: CC-BY-3.0 (SPDX)
- **Citation**: Cybulska & Vossen (2014)
- **Paper**: <https://aclanthology.org/L14-1646/>
- **Notes**: De facto CDCR standard; topic-clustered structure may cause overfitting
- **URL**: <https://raw.githubusercontent.com/cltl/ecbPlus/master/ECB%2B_LREC2014/ECBplus_coreference_sentences.csv>

**Example**:
```
Doc1: 'The earthquake [struck] at 3am.' Doc2: 'The [tremor] caused damage.'
Events: struck_1, tremor_2 -> coreferent (same event)
```

### OntoNotes Coreference

**Rust ID**: `DatasetId::OntoNotesCoref`

OntoNotes 5.0 coreference annotations. Gold-standard multi-genre coref including WSJ, broadcast, web.

- **Language**: en
- **Domain**: mixed
- **Entity Types**: PER, ORG, GPE, NORP
- **Year**: 2012
- **Format**: CoNLL
- **Annotation Scheme**: CoNLLCoref
- **Size**: 3,493 documents, ~1.6M tokens
- **License**: LDC
- **Citation**: Pradhan et al. (2012)
- **Paper**: <https://aclanthology.org/W12-4501/>
- **Notes**: De facto standard for within-document coreference evaluation
- **URL**: <https://catalog.ldc.upenn.edu/LDC2013T19>

### WikiCoref

**Rust ID**: `DatasetId::WikiCoref`

Wikipedia coreference corpus. 30 documents with full coreference annotation.

- **Language**: en
- **Domain**: wikipedia
- **Entity Types**: PER, ORG, LOC
- **Year**: 2016
- **Format**: CoNLL
- **Size**: 30 documents, ~60k tokens
- **License**: CC-BY-SA-4.0 (SPDX)
- **Citation**: Ghaddar & Langlais (2016)
- **Paper**: <https://aclanthology.org/C16-1252/>
- **Notes**: Long documents averaging 2k tokens; challenging for span-based models
- **URL**: <https://rali.iro.umontreal.ca/rali/en/wikicoref-corpus>

### ARRAU 3.0

**Rust ID**: `DatasetId::ARRAU3`

Anaphora Resolution and Underspecification corpus version 3. Multi-genre with rich annotation.

- **Language**: en
- **Domain**: mixed
- **Entity Types**: PER, ORG, LOC, Event
- **Year**: 2024
- **Format**: MMAX2
- **Annotation Scheme**: ARRAU
- **Size**: ~350k tokens across multiple genres
- **License**: Research
- **Citation**: Uryupina et al. (2024)
- **Paper**: <https://aclanthology.org/2024.codi-1.12/>
- **Notes**: Rich annotation including bridging, discourse deixis, and ambiguity
- **URL**: <https://aclanthology.org/2024.codi-1.12/>

### AMI Meeting

**Rust ID**: `DatasetId::AMIMeeting`

Meeting transcripts with coreference and dialogue act annotation.

- **Language**: en
- **Domain**: dialogue
- **Entity Types**: PER, ORG, LOC
- **Year**: 2005
- **Format**: XML
- **Size**: 100 hours of meetings
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Carletta et al. (2005)
- **Paper**: <https://groups.inf.ed.ac.uk/ami/icsi/>
- **Notes**: Multi-party dialogue; includes prosody and head gestures
- **URL**: <https://groups.inf.ed.ac.uk/ami/download/>

### CLEF Clinical Coreference

**Rust ID**: `DatasetId::CLEFClinicalCoref`

Clinical coreference from ShARe/CLEF eHealth. Patient records with coref.

- **Language**: en
- **Domain**: clinical
- **Entity Types**: Disorder, Drug, Procedure
- **Year**: 2013
- **Format**: Standoff
- **Size**: 298 discharge summaries
- **License**: PhysioNet
- **Citation**: Suominen et al. (2013)
- **Paper**: <https://clef2013.clef-initiative.eu/index.php?page=pages/proceedings.php>
- **Notes**: Clinical concept coreference; disorder mentions across sentences
- **URL**: <https://physionet.org/content/shareclefehealth2013coreference/>

### RST Discourse Treebank

**Rust ID**: `DatasetId::RSTDT`

Penn Discourse Treebank with RST annotations. Discourse relations and structure.

- **Language**: en
- **Domain**: news
- **Year**: 2001
- **Format**: Custom
- **Size**: 385 WSJ articles
- **License**: LDC
- **Citation**: Carlson et al. (2001)
- **Paper**: <https://aclanthology.org/A00-1036/>
- **Notes**: RST discourse structure; useful for discourse-aware coreference
- **URL**: <https://catalog.ldc.upenn.edu/LDC2002T07>

### WinoBias

**Rust ID**: `DatasetId::WinoBias`

Coreference bias benchmark. Winograd-schema sentences testing occupational gender stereotypes.

- **Language**: en
- **Domain**: evaluation
- **Entity Types**: PER
- **Year**: 2018
- **Format**: Custom
- **Size**: 3,160 sentences
- **License**: MIT (SPDX)
- **Citation**: Zhao et al. (2018)
- **Paper**: <https://aclanthology.org/N18-2003/>
- **Notes**: Type 1 (syntactic) and Type 2 (semantic) splits; tests BLS occupational stats
- **URL**: <https://raw.githubusercontent.com/uclanlp/corefBias/master/WinoBias/wino/data/anti_stereotyped_type1.txt.dev>

### qxoRef

**Rust ID**: `DatasetId::QxoRef`

First coreference corpus for Conchucos Quechua. Historically significant as first indigenous coref resource.

- **Language**: qxo
- **Domain**: narrative
- **Entity Types**: PER, LOC, ORG
- **Year**: 2021
- **Format**: CoNLL
- **Annotation Scheme**: CoNLLCoref
- **Size**: 12 docs, 332 mentions
- **License**: CC-BY-NC-SA-4.0 (SPDX)
- **Citation**: Rios (2021)
- **Paper**: <https://aclanthology.org/2021.americasnlp-1.1/>
- **Notes**: First indigenous coreference corpus; pro-drop language; agglutinative morphology
- **URL**: <https://raw.githubusercontent.com/Lguyogiro/qxoRef/main/data/conll/all.conll>

### AmericasNLI

**Rust ID**: `DatasetId::AmericasNLI`

NLI for 10 Indigenous American languages (Quechua, Guaraní, Nahuatl, etc.).

- **Language**: multi
- **Domain**: general
- **Year**: 2022
- **Format**: TSV
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Ebrahimi et al. (2022)
- **Paper**: <https://aclanthology.org/2022.acl-long.435/>
- **Notes**: Tests zero-shot transfer from multilingual models; 10 indigenous languages
- **URL**: <https://raw.githubusercontent.com/nala-cub/AmericasNLI/main/data/test/quechua_test.tsv>

### Cherokee NER

**Rust ID**: `DatasetId::CherokeeNER`

Cherokee-English parallel corpus for NER transfer. Uses Syllabary script.

- **Language**: chr
- **Domain**: general
- **Entity Types**: PER, LOC, ORG
- **Year**: 2020
- **Format**: Custom
- **License**: Research
- **Citation**: Zhang et al. (2020)
- **Paper**: <https://aclanthology.org/2020.findings-emnlp.464/>
- **Notes**: Syllabary script (85 characters); polysynthetic language; ~7k speakers
- **URL**: <https://raw.githubusercontent.com/ZhangShiyworkhub/ChrEn/main/chr/chr.txt>

### Nahuatl NER

**Rust ID**: `DatasetId::NahuatlNER`

Named entity recognition for Nahuatl (Aztec language). Colonial-era texts and modern usage.

- **Language**: nah
- **Domain**: historical
- **Entity Types**: PER, LOC, ORG
- **Year**: 2023
- **Format**: CoNLL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Gutierrez-Vasquez et al. (2023)
- **Notes**: Polysynthetic Uto-Aztecan language; ~1.7M speakers; includes colonial manuscripts
- **URL**: <https://github.com/Lguyogiro/nahuatl-ner>

### Māori NER

**Rust ID**: `DatasetId::MaoriNER`

Named entity recognition for Te Reo Māori. New Zealand indigenous language corpus.

- **Language**: mi
- **Domain**: general
- **Entity Types**: PER, LOC, ORG
- **Year**: 2022
- **Format**: JSONL
- **License**: Research
- **Citation**: Te Hiku Media (2022)
- **Notes**: Polynesian language; ~50k fluent speakers; limited training data available
- **URL**: *Requires license or manual download*

### Welsh NER

**Rust ID**: `DatasetId::WelshNER`

Named entity recognition for Welsh (Cymraeg). Celtic language NER corpus.

- **Language**: cy
- **Domain**: news
- **Entity Types**: PER, LOC, ORG, MISC
- **Year**: 2021
- **Format**: CoNLL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Roberts et al. (2021)
- **Notes**: Celtic language; ~900k speakers; supports Welsh-specific entity types
- **URL**: <https://github.com/Portulan/welsh-ner>

### Basque NER

**Rust ID**: `DatasetId::BasqueNER`

Named entity recognition for Basque (Euskara). Language isolate NER corpus.

- **Language**: eu
- **Domain**: news
- **Entity Types**: PER, LOC, ORG, MISC
- **Year**: 2019
- **Format**: CoNLL
- **Size**: ~80k tokens
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Alegria et al. (2019)
- **Notes**: Language isolate; agglutinative morphology; ~750k speakers; ergative-absolutive alignment
- **URL**: <https://github.com/ixa-ehu/eusner>

### HIPE-2022

**Rust ID**: `DatasetId::HIPE2022`

Multilingual Historical NER. 6 datasets across 11 languages including Latin.

- **Language**: multi
- **Domain**: historical
- **Entity Types**: PER, LOC, ORG, PROD
- **Year**: 2022
- **Format**: TSV
- **Annotation Scheme**: IOB2
- **License**: CC-BY-NC-4.0 (SPDX)
- **Citation**: Ehrmann et al. (2022)
- **Paper**: <https://ceur-ws.org/Vol-3180/paper-83.pdf>
- **Notes**: CLEF-HIPE shared task; includes Latin and Classical commentary; OCR noise
- **URL**: <https://raw.githubusercontent.com/hipe-eval/HIPE-2022-data/main/data/v2.1/de/HIPE-2022-v2.1-hipe2020-de-test.tsv>

### HistNERo

**Rust ID**: `DatasetId::HistNERo`

Romanian historical newspaper NER. First Romanian historical NER corpus from four regions.

- **Language**: ro
- **Domain**: historical
- **Entity Types**: PER, LOC, ORG, DATE, MISC
- **Year**: 2024
- **Format**: CoNLL
- **Size**: ~323k tokens, 19th-20th century newspapers
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: HistNERo Team (2024)
- **Paper**: <https://arxiv.org/abs/2405.00155>
- **Notes**: Four historical Romanian regions (Bessarabia, Moldavia, Transylvania, Wallachia); diachronic benchmark
- **URL**: <https://github.com/UniBuc-HistNERo/HistNERo>

### Quaero Old Press

**Rust ID**: `DatasetId::QuaeroOldPress`

French historical newspaper NER from 1890. OCR-corrected with manual NE annotations.

- **Language**: fr
- **Domain**: historical
- **Entity Types**: PER, LOC, ORG, TIME, PROD
- **Year**: 2012
- **Format**: XML
- **Size**: 295 pages, 1890 newspapers
- **License**: Research
- **Citation**: Galibert et al. (2012)
- **Notes**: French historical NER benchmark; manual OCR corrections; reasonably clean historical text
- **URL**: *Requires license or manual download*

### Historical Chinese NER

**Rust ID**: `DatasetId::HistoricalChineseNER`

Multi-task historical Chinese corpus. NER + entity linking + coreference + relations.

- **Language**: zh
- **Domain**: historical
- **Entity Types**: PER, LOC, ORG, TIME, OFFICIAL
- **Year**: 2024
- **Format**: JSONL
- **Size**: Historical Chinese newspapers + documents
- **License**: Research
- **Citation**: LREC-COLING (2024)
- **Paper**: <https://aclanthology.org/2024.lrec-main.35.pdf>
- **Notes**: LREC-COLING 2024; multi-task historical IE benchmark; cross-genre historical Chinese
- **URL**: *Requires license or manual download*

### CHisIEC

**Rust ID**: `DatasetId::CHisIEC`

Chinese Historical Information Extraction Corpus. Ancient Chinese NER + RE with 12 relation types.

- **Language**: lzh
- **Domain**: historical
- **Entity Types**: PER, LOC, OFI, BOOK
- **Year**: 2024
- **Format**: JSON
- **Size**: 3,891 paragraphs, 13,520 entities, 8,228 relations
- **License**: Research
- **Citation**: Tang et al. (2024)
- **Paper**: <https://aclanthology.org/2024.lrec-main.283/>
- **Notes**: Ancient Chinese dynastic histories (24史); 12 domain-specific relations for historical socio-political structures; pre-modern Chinese (文言文)
- **URL**: <https://raw.githubusercontent.com/tangxuemei1995/CHisIEC/main/data/re/coling_test.json>

### DocRED

**Rust ID**: `DatasetId::DocRED`

Document-level relation extraction. 96 relation types from Wikipedia.

- **Language**: en
- **Domain**: wikipedia
- **Entity Types**: PER, LOC, ORG, TIME, NUM
- **Year**: 2019
- **Format**: JSONL
- **Size**: 5,053 docs, 132k entities, 56k relations
- **License**: MIT (SPDX)
- **Citation**: Yao et al. (2019)
- **Paper**: <https://aclanthology.org/P19-1074/>
- **URL**: <https://raw.githubusercontent.com/mainlp/CrossRE/main/crossre_data/ai-test.json>

### Re-TACRED

**Rust ID**: `DatasetId::ReTACRED`

Large-scale relation extraction. 41 relation types + no_relation. Cleaned TACRED.

- **Language**: en
- **Domain**: news
- **Entity Types**: PER, ORG, LOC, DATE, NUM
- **Year**: 2021
- **Format**: JSONL
- **Size**: ~106k relations
- **License**: LDC
- **Citation**: Stoica et al. (2021)
- **Paper**: <https://aclanthology.org/2021.acl-long.359/>
- **Notes**: Cleaned version of TACRED with ~23% relabeled; requires original TACRED
- **URL**: <https://raw.githubusercontent.com/mainlp/CrossRE/main/crossre_data/news-test.json>

### ACE 2004

**Rust ID**: `DatasetId::ACE2004`

Nested entity recognition benchmark. Influential early corpus for nested NER research.

- **Language**: en
- **Domain**: news
- **Entity Types**: PER, ORG, GPE, LOC, FAC, WEA, VEH
- **Year**: 2004
- **Format**: XML
- **Annotation Scheme**: Standoff
- **License**: LDC
- **Citation**: Doddington et al. (2004)
- **Paper**: <https://aclanthology.org/L04-1011/>
- **Notes**: Requires LDC license; includes entity relations; ~25% nested entities
- **URL**: *Requires license or manual download*

### CADEC

**Rust ID**: `DatasetId::CADEC`

Clinical Adverse Drug Events. Benchmark for discontinuous NER from AskaPatient.

- **Language**: en
- **Domain**: biomedical
- **Entity Types**: ADR, Drug, Disease, Symptom
- **Year**: 2015
- **Format**: BRAT
- **Annotation Scheme**: Standoff
- **Size**: ~1,250 posts
- **License**: Research
- **Citation**: Karimi et al. (2015)
- **Paper**: <https://pubmed.ncbi.nlm.nih.gov/25817970/>
- **Notes**: Discontinuous spans common; requires special handling; patient-written text
- **URL**: <https://huggingface.co/datasets/KevinSpaghetti/cadec>

**Example**:
```
'severe [pain]...in my [legs]' -> ADR spans [0:10, 20:24] (discontinuous)
```

### WinoQueer

**Rust ID**: `DatasetId::WinoQueer`

Anti-LGBTQ+ bias benchmark. Community-in-the-loop design for queer representation.

- **Language**: en
- **Domain**: evaluation
- **Entity Types**: PER
- **Year**: 2023
- **Format**: CSV
- **Size**: 45,540 sentence pairs, ~4.8MB
- **License**: MIT (SPDX)
- **Citation**: Felkner et al. (2023)
- **Paper**: <https://aclanthology.org/2023.acl-long.507/>
- **Notes**: Community-designed; tests queer stereotypes in LLMs; Winograd-schema style
- **URL**: <https://raw.githubusercontent.com/katyfelkner/winoqueer/main/data/winoqueer_final.csv>

### BBQ

**Rust ID**: `DatasetId::BBQ`

Bias Benchmark for QA. Tests 9 social bias categories including sexual orientation.

- **Language**: en
- **Domain**: evaluation
- **Year**: 2022
- **Format**: JSONL
- **Size**: ~58k QA pairs across 11 categories
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Parrish et al. (2022)
- **Paper**: <https://aclanthology.org/2022.findings-acl.165/>
- **Notes**: Hand-built ambiguous contexts; age, disability, nationality, religion, etc.
- **URL**: <https://raw.githubusercontent.com/nyu-mll/BBQ/main/data/Gender_identity.jsonl>

### GICoref

**Rust ID**: `DatasetId::GICoref`

Gender-inclusive coreference. Written by/about trans and non-binary individuals.

- **Language**: en
- **Domain**: evaluation
- **Entity Types**: PER
- **Year**: 2020
- **Format**: CoNLL
- **Annotation Scheme**: CoNLLCoref
- **Size**: 95 docs, 470KB
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Cao & Daume III (2020)
- **Paper**: <https://aclanthology.org/2020.acl-main.418/>
- **Notes**: Includes neopronouns (ze/hir, xe/xem); singular they; first gender-inclusive coref corpus
- **URL**: <https://raw.githubusercontent.com/TristaCao/into_inclusivecoref/master/GICoref/coref.combo.conll>

### CorefUD

**Rust ID**: `DatasetId::CorefUD`

Multilingual coreference (17 languages, 22 datasets). CRAC shared task standard.

- **Language**: multi
- **Domain**: general
- **Entity Types**: PER, LOC, ORG
- **Year**: 2022
- **Format**: CoNLLU
- **Annotation Scheme**: CoNLLCoref
- **License**: CC-BY-NC-SA-4.0 (SPDX)
- **Citation**: Nedoluzhko et al. (2022)
- **Paper**: <https://aclanthology.org/2022.lrec-1.581/>
- **Notes**: CRAC shared task standard; includes zero anaphora; harmonized across treebanks
- **URL**: <https://ufal.mff.cuni.cz/corefud/corefud-1.3.zip>

### TransMuCoRes

**Rust ID**: `DatasetId::TransMuCoRes`

Coreference in 31 South Asian languages. Silver annotations via NLLB-200 translation.

- **Language**: multi
- **Domain**: general
- **Entity Types**: PER, LOC, ORG
- **Year**: 2024
- **Format**: JSONL
- **License**: Research
- **Citation**: Verma et al. (2024)
- **Paper**: <https://arxiv.org/abs/2402.13571>
- **Notes**: Silver annotations via translation; fine-tuned mBERT models available
- **URL**: *Requires license or manual download*

### mGAP

**Rust ID**: `DatasetId::MGAP`

Multilingual Gender-Ambiguous Pronouns. 27 South Asian languages.

- **Language**: multi
- **Domain**: evaluation
- **Entity Types**: PER
- **Year**: 2025
- **Format**: TSV
- **Size**: 8,908 pronoun-name pairs
- **License**: Research
- **Citation**: Verma et al. (2025)
- **Paper**: <https://aclanthology.org/2025.chipsal-1.10/>
- **Notes**: Cross-attention improves results; extension of GAP to South Asian languages
- **URL**: *Requires license or manual download*

### CrowS-Pairs

**Rust ID**: `DatasetId::CrowSPairs`

Crowdsourced stereotype pairs benchmark. 9 bias categories for language models.

- **Language**: en
- **Domain**: evaluation
- **Year**: 2020
- **Format**: CSV
- **Size**: ~1.5k sentence pairs
- **License**: CC-BY-SA-4.0 (SPDX)
- **Citation**: Nangia et al. (2020)
- **Paper**: <https://aclanthology.org/2020.emnlp-main.154/>
- **Notes**: Tests stereotypical associations; gender, race, religion, age, nationality, etc.
- **URL**: <https://github.com/nyu-mll/crows-pairs>

### StereoSet

**Rust ID**: `DatasetId::StereoSet`

Measuring stereotypical bias in language models. 4 target domains.

- **Language**: en
- **Domain**: evaluation
- **Year**: 2020
- **Format**: JSONL
- **Size**: ~17k instances
- **License**: MIT (SPDX)
- **Citation**: Nadeem et al. (2020)
- **Paper**: <https://aclanthology.org/2021.acl-long.416/>
- **Notes**: Intrasentence and intersentence evaluation; gender, profession, race, religion
- **URL**: <https://github.com/moinnadeem/StereoSet>

### RealToxicityPrompts

**Rust ID**: `DatasetId::RealToxicityPrompts`

100k prompts for measuring toxicity generation in language models.

- **Language**: en
- **Domain**: evaluation
- **Year**: 2020
- **Format**: JSONL
- **Size**: ~100k prompts
- **License**: Apache-2.0 (SPDX)
- **Citation**: Gehman et al. (2020)
- **Paper**: <https://aclanthology.org/2020.findings-emnlp.301/>
- **Notes**: Tests toxicity generation; perspectives API scores; diverse prompt styles
- **URL**: <https://huggingface.co/datasets/allenai/real-toxicity-prompts>

### BOLD

**Rust ID**: `DatasetId::BoldBias`

Bias in Open-ended Language Generation Dataset. Wikipedia-based prompts.

- **Language**: en
- **Domain**: evaluation
- **Year**: 2021
- **Format**: JSONL
- **Size**: ~23k prompts
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Dhamala et al. (2021)
- **Paper**: <https://aclanthology.org/2021.findings-acl.311/>
- **Notes**: Tests generation bias; profession, gender, race, religion, political ideology
- **URL**: <https://github.com/amazon-science/bold>

### DROC

**Rust ID**: `DatasetId::DROC`

German novel coreference. 90 German novels from DTA (Deutsches Textarchiv).

- **Language**: de
- **Domain**: literature
- **Entity Types**: PER
- **Year**: 2018
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Krug et al. (2018)
- **Paper**: <https://aclanthology.org/L18-1045/>
- **Notes**: First public German literary coreference dataset
- **URL**: <https://raw.githubusercontent.com/dbamman/droc/main/data/dta_reduced.jsonl>

### FantasyCoref

**Rust ID**: `DatasetId::FantasyCoref`

Fantasy fiction coreference. Handles entity transformations.

- **Language**: en
- **Domain**: literature
- **Entity Types**: PER, LOC, ORG
- **License**: Research
- **Citation**: Shin et al. (2023)
- **Paper**: <https://aclanthology.org/2023.tacl-1.52/>
- **Notes**: Shape-shifting, possession, disguise - unique challenges
- **URL**: *Requires license or manual download*

### BOOKCOREF

**Rust ID**: `DatasetId::BookCoref`

Book-scale coreference. First benchmark with 200k+ tokens/doc average. Character coreference on 53 Project Gutenberg novels.

- **Language**: en
- **Domain**: literature
- **Entity Types**: PER
- **Year**: 2025
- **Format**: Custom
- **Annotation Scheme**: CoNLLCoref
- **Size**: 53 books, ~10.8M tokens silver, 229k tokens gold
- **License**: CC-BY-NC-SA-4.0 (SPDX)
- **Citation**: Martinelli et al. (2025)
- **Paper**: <https://aclanthology.org/2025.acl-long.1197/>
- **Notes**: Gold test set: 3 books (Animal Farm, Siddhartha, Pride & Prejudice). Silver train: 45 books. Unprecedented 73k avg mention distance. Current systems drop ~15 CoNLL F1 from windowed to full-book eval.
- **URL**: <https://huggingface.co/datasets/sapienzanlp/bookcoref>

**Example**:
```
doc_key: pride_and_prejudice_1342
sentences: [[CHAPTER, I.], [It, is, a, truth, ...]]
clusters: [[[79,80], [81,82], ...], ...]
characters: [{name: Mr Bennet, cluster: [[79,80]]}]
```

### BOOKCOREF (Split)

**Rust ID**: `DatasetId::BookCorefSplit`

BOOKCOREF split into 1500-token windows for comparison with standard benchmarks.

- **Language**: en
- **Domain**: literature
- **Entity Types**: PER
- **Year**: 2025
- **Format**: Custom
- **Annotation Scheme**: CoNLLCoref
- **Size**: 7544 train, 398 val, 152 test windows
- **License**: CC-BY-NC-SA-4.0 (SPDX)
- **Citation**: Martinelli et al. (2025)
- **Paper**: <https://aclanthology.org/2025.acl-long.1197/>
- **Notes**: Same data as BOOKCOREF but windowed. Enables fair comparison: Maverickxl gets 82.2 CoNLL F1 on split vs 61.0 on full books.
- **URL**: <https://huggingface.co/datasets/sapienzanlp/bookcoref>

### LongtoNotes

**Rust ID**: `DatasetId::LongtoNotes`

OntoNotes with merged coreference chains. Manually merges split OntoNotes documents back into full documents.

- **Language**: en
- **Domain**: mixed
- **Entity Types**: PER, ORG, GPE, NORP
- **Year**: 2023
- **Format**: CoNLL
- **Annotation Scheme**: CoNLLCoref
- **Size**: 2,415 documents, up to 8x longer than OntoNotes
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Shridhar et al. (2023)
- **Paper**: <https://aclanthology.org/2023.findings-eacl.105/>
- **Notes**: Requires OntoNotes access. Documents up to 8x OntoNotes length, 2x LitBank. Multi-genre (WSJ, broadcast, web).
- **URL**: <https://docs.google.com/forms/d/e/1FAIpQLScoWkBOgJ1HH_phtvTJ4_hGvQw6f0W6K7kw74sUKCDTG8P2iA/viewform>

### MovieCoref

**Rust ID**: `DatasetId::MovieCoref`

Screenplay coreference. Character coreference in movie screenplays with unique structural challenges.

- **Language**: en
- **Domain**: literature
- **Entity Types**: PER
- **Year**: 2021
- **Format**: CoNLL
- **Annotation Scheme**: CoNLLCoref
- **Size**: 9 screenplays (~22k tokens/doc avg), 6 full + 3 excerpts
- **License**: Research
- **Citation**: Baruah et al. (2021)
- **Paper**: <https://aclanthology.org/2021.findings-acl.176/>
- **Notes**: Screenplay structure (scene headings, character names, parentheticals) differs significantly from prose. Focus on character coreference only.
- **URL**: <https://aclanthology.org/attachments/2021.findings-acl.176.OptionalSupplementaryMaterial.gz>

### TwiConv

**Rust ID**: `DatasetId::TwiConv`

Twitter conversational coreference.

- **Language**: en
- **Domain**: social_media
- **Entity Types**: PER, LOC, ORG
- **License**: Research
- **Citation**: Aktaş et al. (2020)
- **Paper**: <https://aclanthology.org/2020.lrec-1.835/>
- **Notes**: Turn-taking dynamics; speaker grounding
- **URL**: <https://raw.githubusercontent.com/berfingit/TwiConv/main/conll_skeleton/001_940791133357199360.branch7._with_boundaries_gold_conll>

### MuDoCo

**Rust ID**: `DatasetId::MuDoCo`

Multi-domain document-level coreference. Dialog-based.

- **Language**: en
- **Domain**: dialogue
- **Entity Types**: PER, LOC, ORG
- **License**: MIT (SPDX)
- **Citation**: Raghunathan et al. (2020)
- **Paper**: <https://arxiv.org/abs/2005.00816>
- **URL**: <https://raw.githubusercontent.com/facebookresearch/mudoco/main/mudoco_calling.json>

### DialogRE

**Rust ID**: `DatasetId::DialogRE`

Dialogue-based relation extraction. Multi-turn conversations requiring entity tracking across turns.

- **Language**: en
- **Domain**: dialogue
- **Entity Types**: PER, ORG, LOC
- **Year**: 2020
- **Format**: JSONL
- **Size**: ~1.8k dialogues, 36 relation types
- **License**: CC-BY-NC-SA-4.0 (SPDX)
- **Citation**: Yu et al. (2020)
- **Paper**: <https://aclanthology.org/2020.acl-main.444/>
- **Notes**: Based on Friends TV show transcripts; requires tracking entities across dialogue turns
- **URL**: <https://github.com/nlpdata/dialogre>

### MultiWOZ NER

**Rust ID**: `DatasetId::MultiWOZNER`

Multi-domain task-oriented dialogue with slot/entity annotations. Multi-turn conversations.

- **Language**: en
- **Domain**: dialogue
- **Entity Types**: RESTAURANT, HOTEL, ATTRACTION, TAXI, TRAIN, HOSPITAL, POLICE
- **Year**: 2018
- **Format**: JSONL
- **Size**: ~10k dialogues, 7 domains
- **License**: Apache-2.0 (SPDX)
- **Citation**: Budzianowski et al. (2018)
- **Paper**: <https://aclanthology.org/D18-1547/>
- **Notes**: Standard benchmark for dialogue state tracking; slot values correspond to entities
- **URL**: <https://github.com/budzianowski/multiwoz>

### CoQA

**Rust ID**: `DatasetId::CoQAEntities`

Conversational Question Answering. Multi-turn QA requiring entity mention resolution.

- **Language**: en
- **Domain**: general
- **Entity Types**: ANSWER_SPAN
- **Year**: 2019
- **Format**: JSONL
- **Size**: ~8k conversations, ~127k QA turns
- **License**: Research
- **Citation**: Reddy et al. (2019)
- **Paper**: <https://aclanthology.org/Q19-1016/>
- **Notes**: Multi-turn QA; implicit entity tracking across conversation history; 7 diverse domains
- **URL**: <https://stanfordnlp.github.io/coqa/>

### Gun Violence Corpus

**Rust ID**: `DatasetId::GVC`

Cross-document event coreference for gun violence. Tests domain transfer from ECB+.

- **Language**: en
- **Domain**: news
- **Entity Types**: EVENT, PARTICIPANT, WEAPON, LOCATION, TIME
- **Year**: 2018
- **Format**: Custom
- **Size**: ~500 docs, 510 mentions
- **License**: Research
- **Citation**: Vossen et al. (2018)
- **Paper**: <https://aclanthology.org/L18-1182/>
- **Notes**: Domain-specific CDEC; requires participant/temporal reasoning unlike lemma-driven ECB+
- **URL**: <https://github.com/cltl/GunViolenceCorpus>

### Football Coreference Corpus

**Rust ID**: `DatasetId::FCC`

Cross-document event coreference for football matches. Requires temporal reasoning.

- **Language**: en
- **Domain**: sports
- **Entity Types**: EVENT, PARTICIPANT, LOC, TIME
- **License**: Research
- **Citation**: Bugert et al. (2021)
- **Paper**: <https://direct.mit.edu/coli/article/47/3/575/102774/>
- **Notes**: Requires temporal reasoning about match dates
- **URL**: *Requires license or manual download*

### ECB+META

**Rust ID**: `DatasetId::ECBPlusMeta`

ECB+ with metaphoric paraphrases. ChatGPT-transformed sentences.

- **Language**: en
- **Domain**: news
- **Entity Types**: EVENT, TIME, LOC, PARTICIPANT
- **License**: Research
- **Citation**: Pouran Ben Veyseh et al. (2024)
- **Paper**: <https://arxiv.org/abs/2407.11988>
- **Notes**: Adversarial; existing systems struggle badly
- **URL**: *Requires license or manual download*

### ARRAU 3.0 (v2)

**Rust ID**: `DatasetId::ARRAU`

Multi-genre anaphoric annotation: identity, bridging, discourse deixis, split antecedents.

- **Language**: en
- **Domain**: general
- **Entity Types**: PER, LOC, ORG, EVENT
- **Year**: 2024
- **Format**: MMAX2
- **License**: LDC + Research
- **Citation**: Poesio et al. (2024)
- **Paper**: <https://aclanthology.org/2024.codi-1.12/>
- **Notes**: Most comprehensive anaphora resource; RST/TRAINS/Pear/GENIA subsets; LDC2023T05
- **URL**: *Requires license or manual download*

### ISNotes

**Rust ID**: `DatasetId::ISNotes`

Unrestricted bridging anaphora on OntoNotes. ~660 bridging pairs.

- **Language**: en
- **Domain**: news
- **Entity Types**: PER, LOC, ORG
- **License**: Research
- **Citation**: Hou et al. (2018)
- **Paper**: <https://direct.mit.edu/coli/article/44/2/237/1596/>
- **Notes**: Part-whole, set-member, and other bridging relations
- **URL**: <https://www.h-its.org/software/isnotes-corpus/>

### Shell Nouns (ASN)

**Rust ID**: `DatasetId::ShellNouns`

Anaphoric shell noun resolution. 670 English shell nouns from Schmid taxonomy.

- **Language**: en
- **Domain**: general
- **License**: Research
- **Citation**: Kolhatkar & Hirst (2012)
- **Paper**: <https://aclanthology.org/D12-1036/>
- **Notes**: Factual, linguistic, mental, modal, eventive categories
- **URL**: *Requires license or manual download*

### PDTB 3.0

**Rust ID**: `DatasetId::PDTBv3`

Penn Discourse TreeBank v3. 43 discourse relation types.

- **Language**: en
- **Domain**: news
- **License**: LDC
- **Citation**: Prasad et al. (2019)
- **Paper**: <https://catalog.ldc.upenn.edu/LDC2019T05>
- **Notes**: Shallow discourse parsing; connective-argument pairs
- **URL**: *Requires license or manual download*

### CODI-CRAC Bridging

**Rust ID**: `DatasetId::CODICRACBridging`

Universal Anaphora bridging annotations. One of the largest bridging datasets.

- **Language**: en
- **Domain**: dialogue
- **Entity Types**: BRIDGING_REF
- **Year**: 2022
- **Format**: CoNLLUA
- **Size**: AMI, LIGHT, PERSUASION subsets
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: CODI-CRAC (2022)
- **Paper**: <https://aclanthology.org/2024.lrec-main.1484.pdf>
- **Notes**: Dialogue-heavy; extensive bridging annotations; Universal Anaphora format
- **URL**: <https://github.com/UniversalAnaphora/UA-CODI-CRAC>

### Anaphora Accessibility

**Rust ID**: `DatasetId::AnaphoraAccessibility`

Discourse anaphora accessibility evaluation. Tests non-NP antecedents.

- **Language**: en
- **Domain**: evaluation
- **Entity Types**: DISCOURSE_DEIXIS, EVENT_ANAPHORA, CLAUSAL_ANTECEDENT
- **Year**: 2025
- **Format**: JSONL
- **Size**: Controlled evaluation set
- **License**: Research
- **Citation**: Accessibility Authors (2025)
- **Paper**: <https://arxiv.org/html/2502.14119v1>
- **Notes**: 2025 evaluation dataset; focuses on discourse-level anaphora understanding; non-nominal antecedents
- **URL**: *Requires license or manual download*

### Ancient Greek UD

**Rust ID**: `DatasetId::AncientGreekUD`

Universal Dependencies for Ancient Greek. Homeric through Byzantine.

- **Language**: grc
- **Domain**: literature
- **Entity Types**: PER, LOC, ORG
- **Year**: 2016
- **Format**: CoNLLU
- **License**: CC-BY-NC-SA-3.0 (SPDX)
- **Citation**: Celano et al. (2016)
- **Paper**: <https://aclanthology.org/L16-1158/>
- **Notes**: Perseus treebank; spans 1500+ years of Greek; Homeric to Byzantine
- **URL**: <https://raw.githubusercontent.com/UniversalDependencies/UD_Ancient_Greek-Perseus/master/grc_perseus-ud-test.conllu>

**Example**:
```
# text = μῆνιν ἄειδε θεὰ Πηληϊάδεω Ἀχιλῆος
1	μῆνιν	μῆνις	NOUN	_	_	2	obj	_	O
5	Ἀχιλῆος	Ἀχιλλεύς	PROPN	_	_	4	nmod	_	B-PER
```

### Latin UD

**Rust ID**: `DatasetId::LatinUD`

Universal Dependencies for Latin. Classical through Medieval.

- **Language**: la
- **Domain**: literature
- **Entity Types**: PER, LOC, ORG
- **License**: CC-BY-NC-SA-3.0 (SPDX)
- **Citation**: Passarotti et al. (2017)
- **Paper**: <https://aclanthology.org/W17-6526/>
- **Notes**: Index Thomisticus treebank; medieval scholastic
- **URL**: <https://raw.githubusercontent.com/UniversalDependencies/UD_Latin-ITTB/master/la_ittb-ud-test.conllu>

### Coptic Scriptorium

**Rust ID**: `DatasetId::CopticScriptorium`

Sahidic Coptic with multi-layer annotation. ~50k tokens.

- **Language**: cop
- **Domain**: religious
- **Entity Types**: PER, LOC, ORG
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Zeldes & Schroeder (2016)
- **Paper**: <https://aclanthology.org/L16-1313/>
- **Notes**: Multi-layer: morphology, syntax, entities, coreference
- **URL**: <https://data.copticscriptorium.org/>

### LT4HALA Hebrew

**Rust ID**: `DatasetId::LT4HALA`

Biblical Hebrew NER and coreference annotation.

- **Language**: hbo
- **Domain**: religious
- **Entity Types**: PER, LOC, ORG, GPE
- **License**: Research
- **Citation**: LREC-COLING 2024 LT4HALA Workshop
- **Paper**: <https://lt4hala2024.github.io/>
- **Notes**: First systematic biblical Hebrew NER+coref
- **URL**: *Requires license or manual download*

### ORACC

**Rust ID**: `DatasetId::ORACC`

Open Richly Annotated Cuneiform Corpus. Sumerian, Akkadian, Urartian.

- **Language**: akk
- **Domain**: historical
- **Entity Types**: PER, LOC, ORG, DIVINE
- **License**: CC-BY-SA-3.0 (SPDX)
- **Citation**: ORACC Project
- **Paper**: <http://oracc.museum.upenn.edu/doc/about/index.html>
- **Notes**: Cuneiform; logographic+syllabic; polyphony challenges
- **URL**: <http://oracc.museum.upenn.edu/>

### MasakhaNER

**Rust ID**: `DatasetId::MasakhaNER`

NER for 10 African languages. PER/LOC/ORG/DATE.

- **Language**: multi
- **Domain**: news
- **Entity Types**: PER, LOC, ORG, DATE
- **Year**: 2021
- **Format**: CoNLL
- **Annotation Scheme**: BIO
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Adelani et al. (2021)
- **Paper**: <https://aclanthology.org/2021.tacl-1.66/>
- **Notes**: Critically underrepresented languages; community-driven; tonal diacritics preserved
- **URL**: <https://raw.githubusercontent.com/masakhane-io/masakhane-ner/main/data/yor/test.txt>

**Example**:
```
Olúṣẹ́gun B-PER
Obásanjọ́ I-PER
ní O
ìlú O
Abẹ́òkúta B-LOC
. O
```

### MasakhaNER 2.0

**Rust ID**: `DatasetId::MasakhaNER2`

Extended MasakhaNER with 20+ African languages.

- **Language**: multi
- **Domain**: news
- **Entity Types**: PER, LOC, ORG, DATE
- **Year**: 2022
- **Format**: CoNLL
- **Annotation Scheme**: BIO
- **License**: CC-BY-NC-4.0 (SPDX)
- **Citation**: Adelani et al. (2022)
- **Paper**: <https://aclanthology.org/2022.emnlp-main.298/>
- **Notes**: Extended to 20 languages; includes tonal languages
- **URL**: <https://huggingface.co/datasets/masakhane/masakhaner2>

### AfriSenti

**Rust ID**: `DatasetId::AfriSenti`

Sentiment analysis for 14 African languages. 110k+ tweets. SemEval 2023 Task 12.

- **Language**: multi
- **Domain**: social_media
- **Year**: 2023
- **Format**: HuggingFace
- **Size**: ~111k tweets
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Muhammad et al. (2023)
- **Paper**: <https://aclanthology.org/2023.emnlp-main.862/>
- **Notes**: Amharic, Algerian/Moroccan Arabic, Hausa, Igbo, Kinyarwanda, Oromo, Nigerian Pidgin, Mozambican Portuguese, Swahili, Tigrinya, Xitsonga, Twi, Yoruba
- **URL**: <https://huggingface.co/datasets/shmuhammad/AfriSenti-twitter-sentiment>

**Example**:
```
tweet: ይሄው ነው አይደል የእውቀትሽ ጥግ (Amharic)
label: negative
```

### AfriQA

**Rust ID**: `DatasetId::AfriQA`

Cross-lingual QA for 10 African languages. Wikipedia-based.

- **Language**: multi
- **Domain**: wikipedia
- **Year**: 2023
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Ogundepo et al. (2023)
- **Paper**: <https://aclanthology.org/2023.findings-emnlp.997/>
- **Notes**: Cross-lingual retrieval QA; questions in African languages, passages in English/target
- **URL**: <https://huggingface.co/datasets/masakhane/afriqa>

### MasakhaNEWS

**Rust ID**: `DatasetId::MasakhaNEWS`

News topic classification for 16 African languages.

- **Language**: multi
- **Domain**: news
- **Year**: 2023
- **Format**: HuggingFace
- **License**: Apache-2.0 (SPDX)
- **Citation**: Adelani et al. (2023)
- **Paper**: <https://aclanthology.org/2023.acl-long.574/>
- **Notes**: 7 topics: business, entertainment, health, politics, religion, sports, technology
- **URL**: <https://huggingface.co/datasets/masakhane/masakhanews>

### MasakhaPOS

**Rust ID**: `DatasetId::MasakhaPOS`

Part-of-speech tagging for 20 African languages.

- **Language**: multi
- **Domain**: general
- **Year**: 2023
- **Format**: CoNLL-U
- **Annotation Scheme**: IOB2
- **License**: MIT (SPDX)
- **Citation**: Dione et al. (2023)
- **Paper**: <https://aclanthology.org/2023.acl-long.609/>
- **Notes**: Universal Dependencies tagset; includes Bambara, Ewe, Mossi, Chichewa
- **URL**: <https://github.com/masakhane-io/masakhane-pos>

### WikiANN

**Rust ID**: `DatasetId::WikiANN`

Silver-standard NER from Wikipedia hyperlinks. 282 languages.

- **Language**: multi
- **Domain**: wikipedia
- **Entity Types**: PER, LOC, ORG
- **License**: CC-BY-SA-4.0 (SPDX)
- **Citation**: Pan et al. (2017)
- **Paper**: <https://aclanthology.org/P17-1178/>
- **Notes**: Silver annotations; noisy but massive coverage
- **URL**: <https://huggingface.co/datasets/wikiann>

**Example**:
```
tokens: [Berlin, is, the, capital, of, Germany]
ner_tags: [B-LOC, O, O, O, O, B-LOC]
```

### NaijaNER

**Rust ID**: `DatasetId::NaijaNER`

Nigerian Pidgin NER corpus.

- **Language**: pcm
- **Domain**: social_media
- **Entity Types**: PER, LOC, ORG
- **License**: Research
- **Citation**: Oyewusi et al. (2021)
- **Paper**: <https://arxiv.org/abs/2102.05236>
- **Notes**: Nigerian Pidgin English; code-mixing common
- **URL**: *Requires license or manual download*

### WIESP2022-NER (DEAL)

**Rust ID**: `DatasetId::WIESP2022NER`

Astrophysics NER from NASA ADS. 31 entity types: facilities, wavelengths, telescopes, archives.

- **Language**: en
- **Domain**: scientific
- **Entity Types**: WAVELENGTH, TELESCOPE, FACILITY, MODEL, ARCHIVE, DATASET, MISSION
- **Year**: 2022
- **Format**: JSONL
- **Size**: ~3000 annotated abstracts
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Grezes et al. (2022)
- **Paper**: <https://aclanthology.org/2022.wiesp-1.9/>
- **Notes**: AACL-IJCNLP 2022 WIESP shared task; NASA ADS astrophysics literature
- **URL**: <https://huggingface.co/datasets/adsabs/WIESP2022-NER>

### Dutch Archaeology NER

**Rust ID**: `DatasetId::DutchArchaeology`

Archaeological excavation reports from DANS archive. 31k annotations across 6 entity types.

- **Language**: nl
- **Domain**: archaeology
- **Entity Types**: ARTEFACT, PERIOD, MATERIAL, LOCATION, SPECIES, CONTEXT
- **Year**: 2020
- **Format**: CoNLL
- **Size**: ~31k entity annotations, high IAA (0.95)
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Brandsen et al. (2020)
- **Paper**: <https://aclanthology.org/2020.lrec-1.562/>
- **Notes**: Dutch grey literature; basis for ArcheoBERTje model
- **URL**: <https://live.european-language-grid.eu/catalogue/corpus/13410>

### E-NER (EDGAR-NER)

**Rust ID**: `DatasetId::ENer`

NER for US SEC EDGAR filings. 52 documents, 400k+ tokens with legal entities.

- **Language**: en
- **Domain**: legal
- **Entity Types**: PERSON, COURT, BUSINESS, GOVERNMENT, LOCATION, LEGISLATION
- **Year**: 2022
- **Format**: TSV
- **Size**: 52 SEC filings, 400k+ tokens
- **License**: GPL-3.0 (SPDX)
- **Citation**: Au et al. (2022)
- **Paper**: <https://aclanthology.org/2022.nllp-1.22/>
- **Notes**: 10-K, 8-K, prospectuses; CoNLL-style token/tag format
- **URL**: <https://raw.githubusercontent.com/terenceau1/E-NER-Dataset/main/all.csv>

### FINER (Food Ingredients NER)

**Rust ID**: `DatasetId::FINER`

Food ingredient NER from AllRecipes. 182k sentences with ingredient phrases in IOB2 format.

- **Language**: en
- **Domain**: food
- **Entity Types**: INGREDIENT, PRODUCT, QUANTITY, UNIT, STATE
- **Year**: 2022
- **Format**: CoNLL
- **Annotation Scheme**: IOB2
- **Size**: ~182k sentences, ingredient phrases
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Komariah et al. (2022)
- **Paper**: <https://doi.org/10.6084/m9.figshare.20222361>
- **Notes**: Semi-supervised multi-model construction from AllRecipes; RAR archive with CoNLL format
- **URL**: <https://figshare.com/ndownloader/files/36144501>

### AnnoCTR (Cyber Threat Reports)

**Rust ID**: `DatasetId::AnnoCTR`

Cyber threat intelligence NER with MITRE ATT&CK linking. 400 annotated documents from commercial CTI vendors.

- **Language**: en
- **Domain**: cybersecurity
- **Entity Types**: ORGANIZATION, LOCATION, SECTOR, TIME, CODE, THREAT_ACTOR, MALWARE, TOOL, TACTIC, TECHNIQUE
- **Year**: 2024
- **Format**: JSONL
- **Annotation Scheme**: BIO
- **Size**: 400 documents, multi-layer annotation
- **License**: CC-BY-SA-4.0 (SPDX)
- **Citation**: Lange et al. (2024)
- **Paper**: <https://arxiv.org/abs/2404.07765>
- **Notes**: LREC-COLING 2024; links to Wikipedia and MITRE ATT&CK KB; includes entity linking task
- **URL**: <https://github.com/boschresearch/anno-ctr-lrec-coling-2024/archive/refs/heads/main.zip>

### CRAFT

**Rust ID**: `DatasetId::CRAFT`

Colorado Richly Annotated Full-Text. 97 PubMed articles with multi-layer annotation including coreference.

- **Language**: en
- **Domain**: biomedical
- **Entity Types**: GENE, PROTEIN, CHEMICAL, CELL, ORGANISM
- **Year**: 2012
- **Format**: XML
- **Annotation Scheme**: Standoff
- **Size**: 97 full-text articles, ~790k tokens
- **License**: CC-BY-3.0 (SPDX)
- **Citation**: Bada et al. (2012)
- **Paper**: <https://bmcbioinformatics.biomedcentral.com/articles/10.1186/1471-2105-13-161>
- **Notes**: Full-text (not just abstracts); 10 ontologies used for normalization
- **URL**: <https://github.com/UCDenver-ccp/CRAFT/archive/refs/heads/master.zip>

### WNUT-16

**Rust ID**: `DatasetId::WNUT16`

Twitter NER workshop shared task. Focus on rare and emerging entities in noisy social media text.

- **Language**: en
- **Domain**: social_media
- **Entity Types**: PER, LOC, ORG, MISC
- **Year**: 2016
- **Format**: CoNLL
- **Annotation Scheme**: BIO
- **Size**: 3,856 test tweets, 2,394 train
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Strauss et al. (2016)
- **Paper**: <https://aclanthology.org/W16-3919/>
- **Notes**: 89% unseen test entities; predecessor to WNUT-17; harder than standard benchmarks
- **URL**: <https://raw.githubusercontent.com/napsternxg/TwitterNER/master/data/wnut16/test>

### Sanskrit UD

**Rust ID**: `DatasetId::SanskritUD`

Universal Dependencies for Vedic and Classical Sanskrit. Includes Vedas and epics.

- **Language**: sa
- **Domain**: religious
- **Entity Types**: PER, LOC, ORG
- **Year**: 2020
- **Format**: CoNLLU
- **License**: CC-BY-SA-4.0 (SPDX)
- **Citation**: Hellwig et al. (2020)
- **Paper**: <https://aclanthology.org/2020.lrec-1.632/>
- **Notes**: Oldest Indo-European language with extensive NLP resources; Devanagari script
- **URL**: <https://raw.githubusercontent.com/UniversalDependencies/UD_Sanskrit-Vedic/master/sa_vedic-ud-test.conllu>

### Old English UD

**Rust ID**: `DatasetId::OldEnglishUD`

Universal Dependencies for Old English (Anglo-Saxon). York-Toronto-Helsinki corpus.

- **Language**: ang
- **Domain**: historical
- **Entity Types**: PER, LOC, ORG
- **Year**: 2019
- **Format**: CoNLLU
- **License**: CC-BY-SA-4.0 (SPDX)
- **Citation**: Tischler & Walkden (2019)
- **Paper**: <https://aclanthology.org/W19-4214/>
- **Notes**: Anglo-Saxon; insular script variations; 5th-11th century CE
- **URL**: <https://raw.githubusercontent.com/UniversalDependencies/UD_Old_English-YCOE/master/ang_ycoe-ud-test.conllu>

### Old Norse UD

**Rust ID**: `DatasetId::OldNorseUD`

Universal Dependencies for Old Norse/Icelandic Sagas. PROIEL and ISWOC treebanks.

- **Language**: non
- **Domain**: literature
- **Entity Types**: PER, LOC, ORG
- **Year**: 2012
- **Format**: CoNLLU
- **License**: CC-BY-SA-4.0 (SPDX)
- **Citation**: Rögnvaldsson et al. (2012)
- **Paper**: <https://aclanthology.org/L12-1148/>
- **Notes**: Icelandic Sagas and Eddas; runic/Latin script; 9th-14th century CE
- **URL**: <https://raw.githubusercontent.com/UniversalDependencies/UD_Old_Norse-ICEPAHC/master/non_icepahc-ud-test.conllu>

### CALCS-2018

**Rust ID**: `DatasetId::CALCS2018`

Code-Switching Workshop shared task. English-Spanish Twitter NER with 9 entity types.

- **Language**: multi
- **Domain**: social_media
- **Entity Types**: PER, LOC, ORG, GROUP, TITLE, PROD, EVENT, TIME, OTHER
- **Year**: 2018
- **Format**: CoNLL
- **Annotation Scheme**: BIO
- **License**: Research
- **Citation**: Aguilar et al. (2018)
- **Paper**: <https://aclanthology.org/W18-3219/>
- **Notes**: Spanglish; first major code-switching NER shared task
- **URL**: <https://code-switching.github.io/2018/>

### Hinglish NER

**Rust ID**: `DatasetId::HinglishNER`

Hindi-English code-mixed social media NER. Roman script Hindi mixed with English.

- **Language**: multi
- **Domain**: social_media
- **Entity Types**: PER, LOC, ORG
- **Year**: 2020
- **Format**: JSONL
- **Annotation Scheme**: BIO
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Priyadharshini et al. (2020)
- **Paper**: <https://aclanthology.org/2020.calcs-1.6/>
- **Notes**: GLUECoS/LinCE benchmark; download via CodemixedNLP toolkit; Romanized Hindi; ~400M speakers use code-switching daily
- **URL**: <https://github.com/murali1996/CodemixedNLP>

### Medieval Charter NER

**Rust ID**: `DatasetId::MedievalCharterNER`

Multilingual medieval charter NER. Latin, French, Spanish from major charter collections.

- **Language**: multi
- **Domain**: historical
- **Entity Types**: PER, LOC, ORG, DATE
- **Year**: 2022
- **Format**: CoNLL
- **Size**: ~100k tokens across 4 charter collections
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Camps et al. (2022)
- **Paper**: <https://aclanthology.org/2022.lrec-1.530/>
- **Notes**: HOME-ALCAR, CBMA, Diplomata Belgica, CODEA; medieval Latin/vernacular
- **URL**: <https://zenodo.org/records/6463699>

### CBMA Charters

**Rust ID**: `DatasetId::CBMACharters`

Burgundian medieval Latin charters NER. 9th-14th century diplomatic documents.

- **Language**: la
- **Domain**: historical
- **Entity Types**: PER, LOC, ORG, DATE, TITLE
- **Year**: 2021
- **Format**: CoNLL
- **License**: Research
- **Citation**: Perreaux (2021)
- **Paper**: <https://dhq-static.digitalhumanities.org/pdf/000574.pdf>
- **Notes**: Chartae Burgundiae Medii Aevi; medieval Latin; notarial hands
- **URL**: *Requires license or manual download*

### MSNER

**Rust ID**: `DatasetId::MSNER`

Multilingual Spoken NER. Speech-to-NER on VoxPopuli parliamentary speeches.

- **Language**: multi
- **Domain**: speech
- **Entity Types**: PER, LOC, ORG, MISC
- **Year**: 2024
- **Format**: CoNLL
- **Annotation Scheme**: BIO
- **Size**: ~590h train, 17h gold test
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Evain et al. (2024)
- **Paper**: <https://aclanthology.org/2024.isa-1.2/>
- **Notes**: Dutch, French, German, Spanish; ASR transcripts; first multilingual speech NER corpus
- **URL**: <https://rdr.kuleuven.be/dataset.xhtml?persistentId=doi:10.48804/ZTVMIX>

### NoiseBench

**Rust ID**: `DatasetId::NoiseBench`

Robustness benchmark for NER. 6 real noise types: expert, crowd, LLM, distant/weak supervision.

- **Language**: en
- **Domain**: evaluation
- **Entity Types**: PER, LOC, ORG, MISC
- **Year**: 2024
- **Format**: CoNLL
- **Size**: CoNLL-03 subset with 7 label variants
- **License**: MIT (SPDX)
- **Citation**: Merhej et al. (2024)
- **Paper**: <https://aclanthology.org/2024.emnlp-main.1011/>
- **Notes**: Compares simulated vs real label noise; includes German variant
- **URL**: <https://github.com/elenamer/NoiseBench>

### RockNER

**Rust ID**: `DatasetId::RockNER`

Robustness benchmark for NER. Real-world adversarial examples with boundary ambiguity.

- **Language**: en
- **Domain**: evaluation
- **Entity Types**: PER, LOC, ORG, MISC
- **Year**: 2021
- **Format**: CoNLL
- **Size**: ~1.5k challenging examples
- **License**: Apache-2.0 (SPDX)
- **Citation**: Lin et al. (2021)
- **Paper**: <https://aclanthology.org/2021.acl-long.340/>
- **Notes**: ACL 2021; entity boundary attacks, rare entities, syntactic perturbations; robustness stress test
- **URL**: <https://github.com/INK-USC/RockNER>

### CrossWeigh

**Rust ID**: `DatasetId::CrossWeigh`

Cross-lingual adversarial NER evaluation. Tests multilingual model robustness.

- **Language**: multi
- **Domain**: evaluation
- **Entity Types**: PER, LOC, ORG
- **Year**: 2019
- **Format**: CoNLL
- **Size**: Adversarial cross-lingual test sets
- **License**: MIT (SPDX)
- **Citation**: Wang et al. (2019)
- **Paper**: <https://aclanthology.org/D19-1519/>
- **Notes**: Tests cross-lingual transfer robustness; character/word perturbations; zero-shot evaluation
- **URL**: <https://github.com/ZihanWangKi/CrossWeigh>

### ZELDA

**Rust ID**: `DatasetId::ZELDA`

Entity disambiguation benchmark. 95k Wikipedia paragraphs, 8 ED datasets unified.

- **Language**: en
- **Domain**: wikipedia
- **Entity Types**: ENTITY
- **Year**: 2023
- **Format**: JSONL
- **Size**: 95k paragraphs, 825k entities
- **License**: MIT (SPDX)
- **Citation**: Milich & Akbik (2023)
- **Paper**: <https://aclanthology.org/2023.eacl-main.151/>
- **Notes**: Standardized ED evaluation; Wikipedia KB; no emerging entities
- **URL**: <https://github.com/flairNLP/zelda>

### TweetNERD

**Rust ID**: `DatasetId::TweetNERD`

Twitter NER + Entity Linking. End-to-end NERD benchmark spanning 2010-2021.

- **Language**: en
- **Domain**: social_media
- **Entity Types**: PER, LOC, ORG, MISC
- **Year**: 2022
- **Format**: JSONL
- **Size**: 340k+ tweets
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Mishra et al. (2022)
- **Paper**: <https://arxiv.org/abs/2210.08129>
- **Notes**: NeurIPS 2022; temporal drift; NER + EL + end-to-end NERD
- **URL**: <https://zenodo.org/records/6617192>

### AIDA-CoNLL

**Rust ID**: `DatasetId::AIDACoNLL`

Primary entity linking benchmark linking CoNLL-2003 mentions to Wikipedia. De-facto standard for end-to-end EL evaluation.

- **Language**: en
- **Domain**: news
- **Entity Types**: PER, LOC, ORG, MISC
- **Year**: 2011
- **Format**: CoNLL
- **Annotation Scheme**: IOB2
- **Size**: ~1,400 docs, ~34k mentions linked to Wikipedia
- **License**: Research
- **Citation**: Hoffart et al. (2011)
- **Paper**: <https://aclanthology.org/D11-1072/>
- **Notes**: Built on Reuters CoNLL-2003; AIDA-train/A/B splits; foundational EL benchmark; YAGO KB
- **URL**: <https://www.mpi-inf.mpg.de/departments/databases-and-information-systems/research/ambiverse-nlu/aida>

### ACE 2005

**Rust ID**: `DatasetId::ACE2005`

Automatic Content Extraction 2005. Nested NER + relations + events.

- **Language**: en
- **Domain**: news
- **Entity Types**: PER, ORG, GPE, LOC, FAC, WEA, VEH
- **Year**: 2005
- **Format**: XML
- **Annotation Scheme**: Standoff
- **Size**: ~600 documents
- **License**: LDC
- **Citation**: Walker et al. (2006)
- **Paper**: <https://catalog.ldc.upenn.edu/LDC2006T06>
- **Notes**: Gold standard for nested NER; includes Arabic/Chinese; defines modern IE evaluation
- **URL**: *Requires license or manual download*

### NNE (Nested Named Entities)

**Rust ID**: `DatasetId::NNE`

Large-scale nested NER corpus from Wikipedia/news. Deep nesting up to 6 levels.

- **Language**: en
- **Domain**: news
- **Entity Types**: PER, LOC, ORG, GPE, NORP, FAC, PRODUCT, EVENT, WORK, LAW
- **Year**: 2019
- **Format**: CoNLL
- **Size**: ~280k tokens, deep nesting
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Ringland et al. (2019)
- **Paper**: <https://aclanthology.org/P19-1510/>
- **Notes**: ACL 2019; based on ACE/OntoNotes; up to 6 nested levels; stress test for nested NER
- **URL**: <https://github.com/nickyringland/nested_named_entities>

### GENIA Nested

**Rust ID**: `DatasetId::GENIANested`

Biomedical nested NER from GENIA corpus. Up to 3 levels of nesting.

- **Language**: en
- **Domain**: biomedical
- **Entity Types**: DNA, RNA, PROTEIN, CELL_LINE, CELL_TYPE
- **Year**: 2003
- **Format**: CoNLL
- **Size**: ~2k abstracts
- **License**: GENIA Project License
- **Citation**: Kim et al. (2003)
- **Paper**: <https://aclanthology.org/W03-1302/>
- **Notes**: Canonical biomedical nested NER benchmark; used alongside ACE for nested NER evaluation
- **URL**: <https://raw.githubusercontent.com/thecharm/boundary-aware-nested-ner/master/Our_boundary-aware_model/data/genia/genia.test.iob2>

**Example**:
```
[[IL-2 receptor] alpha chain] promoter
[IL-2 receptor]: PROTEIN, [IL-2 receptor alpha chain]: PROTEIN (nested)
```

### Chinese Nested NER

**Rust ID**: `DatasetId::ChineseNestedNER`

Chinese nested named entity recognition. Multiple levels of embedded entities.

- **Language**: zh
- **Domain**: news
- **Entity Types**: PER, ORG, LOC, GPE
- **Year**: 2020
- **Format**: JSONL
- **Size**: ~20k sentences
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Wang et al. (2020)
- **Notes**: Chinese nested NER benchmark; designed for span-based model evaluation; CJK characters
- **URL**: <https://github.com/LeeSureman/Nested-NER>

### SciNER Nested

**Rust ID**: `DatasetId::SCINERNested`

Scientific paper NER with nested annotations. Methods, tasks, and datasets.

- **Language**: en
- **Domain**: scientific
- **Entity Types**: TASK, METHOD, METRIC, MATERIAL, GENERIC
- **Year**: 2018
- **Format**: JSONL
- **Size**: ~500 abstracts
- **License**: Apache-2.0 (SPDX)
- **Citation**: Luan et al. (2018)
- **Paper**: <https://aclanthology.org/D18-1360/>
- **Notes**: Scientific information extraction; nested spans common in methodology descriptions
- **URL**: <https://github.com/allenai/sciie>

### ShARe/CLEF

**Rust ID**: `DatasetId::ShAReCLEF`

Shared Annotated Resources for clinical NER. ShARe/CLEF eHealth shared task.

- **Language**: en
- **Domain**: clinical
- **Entity Types**: DISORDER, FINDING, PROCEDURE
- **Year**: 2013
- **Format**: BRAT
- **Annotation Scheme**: Standoff
- **Size**: ~300 clinical notes
- **License**: PhysioNet
- **Citation**: Pradhan et al. (2013)
- **Paper**: <https://aclanthology.org/S13-2056/>
- **Notes**: Discontinuous clinical entities; SNOMED-CT normalization; de-identified records
- **URL**: *Requires license or manual download*

### GermEval Discontinuous

**Rust ID**: `DatasetId::GermEvalDiscontinuous`

German discontinuous NER from GermEval 2014. Non-contiguous entity spans.

- **Language**: de
- **Domain**: news
- **Entity Types**: PER, ORG, LOC, OTH
- **Year**: 2014
- **Format**: CoNLL
- **Size**: ~87k tokens
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Benikova et al. (2014)
- **Paper**: <https://aclanthology.org/W14-1707/>
- **Notes**: German discontinuous entities; derived entities; embedded entities
- **URL**: <https://sites.google.com/site/germaboreval/data>

### ADR Discontinuous

**Rust ID**: `DatasetId::ADRDiscontinuous`

Adverse Drug Reaction corpus with discontinuous mentions. Patient forum posts.

- **Language**: en
- **Domain**: biomedical
- **Entity Types**: ADR, DRUG, SYMPTOM
- **Year**: 2016
- **Format**: BRAT
- **Size**: ~2k posts
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Metke-Jimenez et al. (2016)
- **Notes**: Social media ADR mentions; many discontinuous spans; health forum text
- **URL**: <https://github.com/Aitslab/ADR-DisNER>

### PubMed Discontinuous

**Rust ID**: `DatasetId::PubMedDiscontinuous`

PubMed abstracts with discontinuous biomedical entities. Complex entity boundaries.

- **Language**: en
- **Domain**: biomedical
- **Entity Types**: CHEMICAL, DISEASE, GENE
- **Year**: 2020
- **Format**: CoNLL
- **Size**: ~8k abstracts
- **License**: Research
- **Citation**: Dai et al. (2020)
- **Notes**: Scientific abstracts; discontinuous chemical and disease mentions
- **URL**: <https://github.com/dmis-lab/discontinuous-ner>

### TACRED

**Rust ID**: `DatasetId::TACRED`

TAC Relation Extraction Dataset. 42 relations from TAC KBP.

- **Language**: en
- **Domain**: news
- **Entity Types**: PER, ORG
- **Year**: 2017
- **Format**: JSONL
- **Size**: 106k examples
- **License**: LDC
- **Citation**: Zhang et al. (2017)
- **Paper**: <https://aclanthology.org/D17-1004/>
- **Notes**: 42 relations; ~80% no_relation; known label noise; Re-TACRED fixes some issues
- **URL**: *Requires license or manual download*

**Example**:
```
subj: 'Tim Cook', obj: 'Apple', relation: per:employee_of, text: 'Tim Cook is the CEO of Apple Inc.'
```

### SemEval-2010 Task 8

**Rust ID**: `DatasetId::SemEval2010Task8`

Semantic relation classification between nominals. 9 relation types.

- **Language**: en
- **Domain**: general
- **Year**: 2010
- **Format**: Custom
- **Size**: ~10k examples
- **License**: Research
- **Citation**: Hendrickx et al. (2010)
- **Paper**: <https://aclanthology.org/S10-1006/>
- **Notes**: Classic RE benchmark; 9 directed relations + OTHER; small but influential
- **URL**: <https://github.com/sahitya0000/Relation-Classification>

### FewRel

**Rust ID**: `DatasetId::FewRel`

Few-shot relation classification benchmark. 100 relations from Wikidata.

- **Language**: en
- **Domain**: wikipedia
- **Year**: 2018
- **Format**: JSONL
- **Size**: 70k instances, 100 relations
- **License**: MIT (SPDX)
- **Citation**: Han et al. (2018)
- **Paper**: <https://aclanthology.org/D18-1514/>
- **Notes**: N-way K-shot evaluation; Wikidata relations; FewRel 2.0 adds domain adaptation
- **URL**: <https://raw.githubusercontent.com/thunlp/FewRel/master/data/val_wiki.json>

### NYT-10

**Rust ID**: `DatasetId::NYT10`

New York Times distant supervision RE. 24 Freebase relations.

- **Language**: en
- **Domain**: news
- **Entity Types**: PER, ORG, LOC
- **Year**: 2010
- **Format**: Custom
- **Size**: ~266k sentences
- **License**: Research
- **Citation**: Riedel et al. (2010)
- **Paper**: <https://aclanthology.org/W10-1001/>
- **Notes**: Distant supervision using Freebase alignment; distantly supervised; noisy labels; ~64% no_relation; standard DS-RE benchmark
- **URL**: <http://iesl.cs.umass.edu/riedel/ecml/>

### JNLPBA

**Rust ID**: `DatasetId::JNLPBA`

JNLPBA 2004 shared task. Bio-entity recognition in PubMed abstracts.

- **Language**: en
- **Domain**: biomedical
- **Entity Types**: PROTEIN, DNA, RNA, CELL_TYPE, CELL_LINE
- **Year**: 2004
- **Format**: CoNLL
- **Annotation Scheme**: IOB2
- **Size**: ~2,400 abstracts
- **License**: Research
- **Citation**: Kim et al. (2004)
- **Paper**: <https://aclanthology.org/W04-1213/>
- **Notes**: Extended GENIA categories; foundational bioNER benchmark
- **URL**: <https://raw.githubusercontent.com/cambridgeltl/MTL-Bioinformatics-2016/master/data/JNLPBA/test.tsv>

### S800

**Rust ID**: `DatasetId::S800`

Species-800 corpus. Species name recognition in biomedical text.

- **Language**: en
- **Domain**: biomedical
- **Entity Types**: SPECIES
- **Year**: 2013
- **Format**: XML
- **Size**: 800 abstracts
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Pafilis et al. (2013)
- **Paper**: <https://journals.plos.org/plosone/article?id=10.1371/journal.pone.0065390>
- **Notes**: Species NER; taxonomy normalization; useful for biodiversity NLP
- **URL**: <https://species.jensenlab.org/files/S800-1.0.tar.gz>

### TempEval-3

**Rust ID**: `DatasetId::TempEval3`

Temporal annotation benchmark. TIMEX, EVENT spans, and temporal relations.

- **Language**: en
- **Domain**: news
- **Entity Types**: TIMEX, EVENT
- **Year**: 2013
- **Format**: TimeML
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: UzZaman et al. (2013)
- **Paper**: <https://aclanthology.org/S13-2001/>
- **Notes**: Time expression NER + event detection + temporal ordering; TimeBank based; TE3-Platinum gold standard
- **URL**: <https://figshare.com/articles/dataset/TempEval-3_data/9586532>

### TimeBank 1.2

**Rust ID**: `DatasetId::TimeBank12`

Canonical temporal IE corpus. News articles with TIMEX3, events, and temporal links (TLINKs).

- **Language**: en
- **Domain**: news
- **Entity Types**: TIMEX3, EVENT, SIGNAL
- **Year**: 2003
- **Format**: TimeML
- **Size**: 183 news documents, ~9k events
- **License**: LDC
- **Citation**: Pustejovsky et al. (2003)
- **Paper**: <https://aclanthology.org/W03-1808/>
- **Notes**: Original TimeML corpus; basis for TempEval shared tasks; temporal ordering gold standard
- **URL**: <https://catalog.ldc.upenn.edu/LDC2006T08>

### MATRES

**Rust ID**: `DatasetId::MATRES`

Multi-Axis Temporal Relations. Cleaner, more consistent event-event temporal relation annotations.

- **Language**: en
- **Domain**: news
- **Entity Types**: EVENT
- **Year**: 2018
- **Format**: Custom
- **Size**: ~13.5k temporal relation pairs
- **License**: Research
- **Citation**: Ning et al. (2018)
- **Paper**: <https://aclanthology.org/P18-1212/>
- **Notes**: Re-annotated TimeBank/AQUAINT subset; higher inter-annotator agreement; verb-centric
- **URL**: <https://github.com/qiangning/MATRES>

### THYME

**Rust ID**: `DatasetId::THYME`

Temporal Histories of Your Medical Events. Clinical temporal IE with events and relations.

- **Language**: en
- **Domain**: clinical
- **Entity Types**: EVENT, TIMEX3, SECTIONTIME, DOCTIME
- **Year**: 2014
- **Format**: Custom
- **Size**: ~600 clinical notes (colon cancer, brain cancer)
- **License**: Research
- **Citation**: Styler et al. (2014)
- **Paper**: <https://aclanthology.org/L14-1393/>
- **Notes**: THYME guidelines; clinical events, temporal expressions, narrative containers; Clinical TempEval basis
- **URL**: *Requires license or manual download*

### i2b2 2012 Temporal

**Rust ID**: `DatasetId::I2B2Temporal`

Clinical temporal relations challenge. Events, TIMEX3, and TLINKs in discharge summaries.

- **Language**: en
- **Domain**: clinical
- **Entity Types**: EVENT, TIMEX3
- **Year**: 2012
- **Format**: Custom
- **Size**: ~310 clinical notes
- **License**: Research
- **Citation**: Sun et al. (2013)
- **Paper**: <https://aclanthology.org/S13-2035/>
- **Notes**: i2b2 2012 challenge; requires DUA; clinical temporal relation extraction benchmark
- **URL**: *Requires license or manual download*

### Twitter-2015 MNER

**Rust ID**: `DatasetId::Twitter2015MNER`

Multimodal NER on Twitter. Text + image for entity recognition.

- **Language**: en
- **Domain**: social_media
- **Entity Types**: PER, LOC, ORG, MISC
- **Year**: 2018
- **Format**: CoNLL
- **Size**: ~8,000 tweets with images
- **License**: Research
- **Citation**: Zhang et al. (2018)
- **Paper**: <https://aclanthology.org/N18-1078/>
- **Notes**: Multimodal; images via Google Drive archive; UMT preprocessing; first MNER dataset; visual context aids entity recognition
- **URL**: <https://github.com/jefferyYu/UMT>

### Distant Listening Corpus

**Rust ID**: `DatasetId::DistantListeningCorpus`

1,283 musical scores with harmonic annotations. String quartet + piano music with Roman numeral analysis.

- **Language**: multi
- **Domain**: music
- **Entity Types**: CHORD, KEY, MODULATION, CADENCE, PHRASE
- **Year**: 2024
- **Format**: TSV
- **Size**: 1,283 scores, 190k+ annotations
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Devaney et al. (2024)
- **Paper**: <https://doi.org/10.5281/zenodo.15150283>
- **Notes**: Music theory annotation corpus; Roman numeral analysis; supports harmonic sequence extraction; Zenodo archive
- **URL**: <https://zenodo.org/records/15150283>

### PII Masking 200k

**Rust ID**: `DatasetId::PIIMasking200k`

200k synthetic examples for PII detection and masking. Covers 50+ PII types.

- **Language**: multi
- **Domain**: privacy
- **Entity Types**: EMAIL, PHONE, SSN, ADDRESS, NAME, DOB, CREDIT_CARD, PASSPORT, IP_ADDRESS, LICENSE
- **Year**: 2024
- **Format**: JSONL
- **Size**: ~200k examples
- **License**: Apache-2.0 (SPDX)
- **Citation**: AI4Privacy (2024)
- **Notes**: Synthetic PII dataset; multi-language; 50+ entity types; useful for privacy compliance testing
- **URL**: <https://huggingface.co/datasets/ai4privacy/pii-masking-200k>

### E-NER SEC

**Rust ID**: `DatasetId::ENERSec`

Legal NER from SEC EDGAR filings. 52 documents with financial entity annotations.

- **Language**: en
- **Domain**: legal
- **Entity Types**: ORG, LOC, DATE, MONEY, PERCENT, PERSON, PRODUCT, CARDINAL
- **Year**: 2023
- **Format**: CSV
- **Size**: 52 documents, ~400k tokens
- **License**: MIT (SPDX)
- **Citation**: Nishii et al. (2023)
- **Notes**: SEC 10-K and 10-Q filings; financial regulatory domain; legal entity extraction
- **URL**: <https://github.com/jnishii/E-NER>

### MSNBC

**Rust ID**: `DatasetId::MSNBCEL`

Small news article entity linking dataset. Commonly used for out-of-domain EL evaluation.

- **Language**: en
- **Domain**: news
- **Entity Types**: PER, LOC, ORG
- **Year**: 2007
- **Format**: Custom
- **Size**: ~20 docs, ~700 mentions
- **License**: Research
- **Citation**: Cucerzan (2007)
- **Paper**: <https://aclanthology.org/D07-1074/>
- **Notes**: Early EL benchmark; often used as OOD test set alongside AIDA
- **URL**: *Requires license or manual download*

### AQUAINT

**Rust ID**: `DatasetId::AQUAINT`

Newswire entity linking dataset from AQUAINT corpus. Wikipedia-linked mentions.

- **Language**: en
- **Domain**: news
- **Entity Types**: PER, LOC, ORG
- **Year**: 2008
- **Format**: Custom
- **Size**: ~50 docs, ~700 mentions
- **License**: LDC
- **Citation**: Milne & Witten (2008)
- **Notes**: Commonly paired with AIDA for comprehensive EL evaluation
- **URL**: *Requires license or manual download*

### KORE50

**Rust ID**: `DatasetId::KORE50`

Short, highly ambiguous entity linking snippets. Tests disambiguation difficulty.

- **Language**: en
- **Domain**: evaluation
- **Entity Types**: PER, LOC, ORG
- **Year**: 2012
- **Format**: Custom
- **Size**: 50 sentences, 144 mentions
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Hoffart et al. (2012)
- **Paper**: <https://aclanthology.org/P12-1084/>
- **Notes**: Highly ambiguous mentions; stress-tests disambiguation ability; includes YAGO types
- **URL**: <https://github.com/KORE50/KORE50-NIF-NER>

### WNED-WIKI

**Rust ID**: `DatasetId::WNEDWiki`

Large-scale Wikipedia entity linking dataset extracted from Wikipedia hyperlinks.

- **Language**: en
- **Domain**: wikipedia
- **Entity Types**: ENTITY
- **Year**: 2018
- **Format**: Custom
- **Size**: ~6M mentions
- **License**: Research
- **Citation**: Guo & Barbosa (2018)
- **Notes**: Large-scale silver annotations from Wikipedia hyperlinks
- **URL**: <https://github.com/wikipedia2vec/wikipedia2vec>

### WNED-ClueWeb

**Rust ID**: `DatasetId::WNEDClueweb`

Web-scale entity linking from ClueWeb corpus. Tests EL on noisy web text.

- **Language**: en
- **Domain**: general
- **Entity Types**: ENTITY
- **Year**: 2018
- **Format**: Custom
- **Size**: ~10k docs
- **License**: Research
- **Citation**: Guo & Barbosa (2018)
- **Notes**: Web-scale EL benchmark; tests robustness on noisy web text
- **URL**: *Requires license or manual download*

### BELB

**Rust ID**: `DatasetId::BELB`

Biomedical Entity Linking Benchmark unifying 11 corpora across 7 knowledge bases. Standardized biomedical EL evaluation.

- **Language**: en
- **Domain**: biomedical
- **Entity Types**: Disease, Chemical, Gene, Species, CellLine, Variant
- **Year**: 2023
- **Format**: JSONL
- **Size**: 11 corpora, 7 KBs
- **License**: Research
- **Citation**: Furrer et al. (2023)
- **Paper**: <https://academic.oup.com/bioinformatics/article/39/11/btad698/7425450>
- **Notes**: Unifies BC5CDR-Chemical, BC5CDR-Disease, NCBI-Disease, BC2GN, NLM-Gene, Linnaeus, S800, GNORMPLUS, MedMentions, and more
- **URL**: <https://github.com/sg-wbi/belb>

### MELO

**Rust ID**: `DatasetId::MELO`

Multilingual Entity Linking of Occupations. 48 datasets across 21 languages for occupation EL.

- **Language**: multi
- **Domain**: general
- **Entity Types**: OCCUPATION
- **Year**: 2024
- **Format**: JSONL
- **Size**: 48 datasets, 21 languages
- **License**: Apache-2.0 (SPDX)
- **Citation**: Retyk et al. (2024)
- **Paper**: <https://aclanthology.org/2024.lrec-main.889/>
- **Notes**: Zero-shot multilingual EL; includes sentence encoders and lexical baselines
- **URL**: <https://github.com/avature/melo-benchmark>

### BookCoref (Bamman)

**Rust ID**: `DatasetId::BookCorefBamman`

Full-novel coreference with automatic silver and manual gold annotations. Includes Animal Farm, Siddhartha, Pride and Prejudice.

- **Language**: en
- **Domain**: literature
- **Entity Types**: PER, LOC, ORG
- **Year**: 2025
- **Format**: JSONL
- **Size**: ~200k tokens per document
- **License**: Research
- **Citation**: Bamman et al. (2025)
- **Paper**: <https://arxiv.org/abs/2507.12075>
- **Notes**: Long-document coref benchmark; tests models on full novels; silver + gold annotations
- **URL**: <https://huggingface.co/datasets/spacemanidol/BookCoref>

### NovelCR

**Rust ID**: `DatasetId::NovelCR`

Large-scale bilingual (EN/ZH) novel coreference. 148k EN mentions, 311k ZH mentions with 74-83% spanning 3+ sentences.

- **Language**: multi
- **Domain**: literature
- **Entity Types**: PER, LOC, ORG
- **Year**: 2024
- **Format**: JSONL
- **Size**: EN: 148k mentions, ZH: 311k mentions
- **License**: Research
- **Citation**: Chen et al. (2024)
- **Paper**: <https://openreview.net/forum?id=zuZXwj9aSE>
- **Notes**: Long-span coreference; bilingual EN/ZH; most coreferences span multiple sentences
- **URL**: <https://github.com/NovelCR/NovelCR>

### AgCNER

**Rust ID**: `DatasetId::AgCNER`

Large-scale Chinese agricultural NER. 66k samples, ~207k entities, 3.9M characters.

- **Language**: zh
- **Domain**: scientific
- **Entity Types**: CROP, DISEASE, PEST, CHEMICAL, VARIETY, LOCATION, TIME
- **Year**: 2024
- **Format**: JSONL
- **Size**: 66k samples, ~207k entities, 3.9M characters
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: AgCNER Team (2024)
- **Paper**: <https://www.nature.com/articles/s41597-024-03578-5>
- **Notes**: Nature Scientific Data 2024; 13 entity types; long agricultural case reports; domain NER
- **URL**: <https://github.com/AgCNER/AgCNER>

### SCROLLS QMSum

**Rust ID**: `DatasetId::ScrollsQMSum`

Long-document QA from SCROLLS benchmark. Query-focused meeting summarization.

- **Language**: en
- **Domain**: dialogue
- **Year**: 2022
- **Format**: JSONL
- **Size**: ~1.5k meeting transcripts, avg 10k tokens
- **License**: MIT (SPDX)
- **Citation**: Shaham et al. (2022)
- **Paper**: <https://aclanthology.org/2022.emnlp-main.823/>
- **Notes**: EMNLP 2022; SCROLLS benchmark subset; long meeting transcripts; tests long-context understanding
- **URL**: <https://github.com/tau-nlp/scrolls>

### Long Document NER

**Rust ID**: `DatasetId::LongDocNER`

Long-document NER benchmark. Tests entity recognition across extended contexts.

- **Language**: en
- **Domain**: general
- **Entity Types**: PER, LOC, ORG, MISC
- **Year**: 2024
- **Format**: JSONL
- **Size**: ~500 documents, avg 8k tokens
- **License**: MIT (SPDX)
- **Citation**: Huang et al. (2024)
- **Notes**: Tests long-context NER models; entity consistency across document boundaries
- **URL**: <https://github.com/xhuang28/LongDocNER>

### BookSum Coref

**Rust ID**: `DatasetId::BookSumCoref`

Coreference annotations on book chapters from BookSum. Long literary texts.

- **Language**: en
- **Domain**: literature
- **Entity Types**: PER, LOC, ORG
- **Year**: 2022
- **Format**: JSONL
- **Size**: ~400 chapters, avg 5k tokens
- **License**: Research
- **Citation**: Kryscinski et al. (2022)
- **Paper**: <https://aclanthology.org/2022.findings-emnlp.438/>
- **Notes**: Book chapters with coref chains; tests long-span coreference resolution
- **URL**: <https://github.com/salesforce/booksum>

### Multi-Bio Long NER

**Rust ID**: `DatasetId::MultiBioNERLong`

Long biomedical document NER. Full-text articles vs abstracts.

- **Language**: en
- **Domain**: biomedical
- **Entity Types**: GENE, CHEMICAL, DISEASE, SPECIES
- **Year**: 2023
- **Format**: JSONL
- **Size**: ~1k full-text articles
- **License**: Research
- **Citation**: Lee et al. (2023)
- **Notes**: Full-text vs abstract NER comparison; tests biomedical long-context models
- **URL**: <https://github.com/dmis-lab/multi-bio-ner>

### RadCoref

**Rust ID**: `DatasetId::RadCoref`

Radiology report coreference from MIMIC-CXR. Clinical domain long-document coref.

- **Language**: en
- **Domain**: clinical
- **Entity Types**: ANATOMY, OBSERVATION, FINDING
- **Year**: 2024
- **Format**: BRAT
- **Size**: ~500 radiology reports
- **License**: PhysioNet
- **Citation**: Zhu et al. (2024)
- **Paper**: <https://physionet.org/content/rad-coreference-resolution/>
- **Notes**: Clinical coref on MIMIC-CXR; requires PhysioNet credentialing; radiology-specific entities
- **URL**: <https://physionet.org/content/rad-coreference-resolution/>

### MEANTIME

**Rust ID**: `DatasetId::MEANTIME`

Multilingual news corpus with within- and cross-document event coreference. 4 languages.

- **Language**: multi
- **Domain**: news
- **Entity Types**: EVENT, TIMEX, PARTICIPANT, LOCATION
- **Year**: 2016
- **Format**: Custom
- **Size**: 120 documents, 4 languages (EN, ES, IT, NL)
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Minard et al. (2016)
- **Paper**: <https://aclanthology.org/L16-1699/>
- **Notes**: Multilingual CDEC; parallel annotations across languages; NewsReader project
- **URL**: <https://github.com/newsreader/meantime>

### FCC-T

**Rust ID**: `DatasetId::FCCT`

Football Coreference Corpus with token-level annotations. Cross-document event coref in sports news.

- **Language**: en
- **Domain**: sports
- **Entity Types**: EVENT, PARTICIPANT, TIME, LOCATION
- **Year**: 2021
- **Format**: CoNLL
- **Size**: ~300 docs
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Bugert et al. (2021)
- **Paper**: <https://direct.mit.edu/coli/article/47/3/575/102774>
- **Notes**: Token-level CDEC; compatible with ECB+ and GVC; sports domain temporal reasoning
- **URL**: <https://github.com/cltl/FCC>

### LEMONADE

**Rust ID**: `DatasetId::LEMONADE`

Large-scale multilingual conflict event corpus. 39k events across 20 languages for CDEC search.

- **Language**: multi
- **Domain**: news
- **Entity Types**: EVENT, PARTICIPANT, LOCATION, TIME
- **Year**: 2025
- **Format**: JSONL
- **Size**: ~39k events, 20 languages, 171 countries
- **License**: Research
- **Citation**: Eirew et al. (2025)
- **Notes**: Conflict event CDEC; cross-document event coreference search task; multilingual
- **URL**: <https://github.com/lemonade-coref/lemonade>

### BioRED

**Rust ID**: `DatasetId::BioRED`

Document-level biomedical RE with novelty labels. BioCreative VIII shared task benchmark.

- **Language**: en
- **Domain**: biomedical
- **Entity Types**: Gene, Disease, Chemical, Species, Variant, CellLine
- **Year**: 2022
- **Format**: Custom
- **Size**: 600 PubMed abstracts, 8 relation types
- **License**: Public (SPDX)
- **Citation**: Luo et al. (2022)
- **Paper**: <https://academic.oup.com/database/article/doi/10.1093/database/baae069/7729400>
- **Notes**: Document-level RE with novelty detection; distinguishes novel vs known relations
- **URL**: <https://ftp.ncbi.nlm.nih.gov/pub/lu/BioRED/>

### MedMentions

**Rust ID**: `DatasetId::MedMentions`

Large-scale biomedical concept mentions mapped to UMLS. PubMed abstracts with fine-grained semantic types.

- **Language**: en
- **Domain**: biomedical
- **Entity Types**: UMLS_CONCEPT
- **Year**: 2019
- **Format**: Custom
- **Size**: 4,392 abstracts, 352k mentions, 35k concepts
- **License**: CC0-1.0
- **Citation**: Mohan & Li (2019)
- **Paper**: <https://arxiv.org/abs/1902.09476>
- **Notes**: UMLS concept linking; 127 semantic types; large-scale biomedical concept NER/EL
- **URL**: <https://github.com/chanzuckerberg/MedMentions>

### EnzChemRED

**Rust ID**: `DatasetId::EnzChemRED`

Enzyme chemistry relation extraction. Links enzymes, substrates, products, cofactors from biochemical literature.

- **Language**: en
- **Domain**: biomedical
- **Entity Types**: Enzyme, Substrate, Product, Cofactor, Reaction
- **Year**: 2024
- **Format**: JSONL
- **Size**: ~5k relation triplets
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Schröder et al. (2024)
- **Paper**: <https://www.nature.com/articles/s41597-024-03835-7>
- **Notes**: Specialized enzyme chemistry RE; biochemical reaction extraction
- **URL**: <https://github.com/EnzChemRED/EnzChemRED>

### NCERB

**Rust ID**: `DatasetId::NCERB`

Named Clinical Entity Recognition Benchmark. Multi-dataset clinical NER evaluation suite.

- **Language**: en
- **Domain**: clinical
- **Entity Types**: Problem, Treatment, Test, Medication, Anatomy
- **Year**: 2024
- **Format**: Custom
- **Size**: Multiple clinical corpora aggregated
- **License**: Research
- **Citation**: Zhou et al. (2024)
- **Paper**: <https://arxiv.org/abs/2410.05046>
- **Notes**: Benchmark suite for clinical NER; evaluates LMs on healthcare entities; aggregates i2b2, n2c2, etc.
- **URL**: <https://github.com/NCERB/NCERB>

### MACCROBAT

**Rust ID**: `DatasetId::MACCROBAT`

Biomedical NER corpus with extensive coverage. Used with RoBERTa-WWM and deep models.

- **Language**: en
- **Domain**: biomedical
- **Entity Types**: Disease, Chemical, Gene, Species
- **Year**: 2019
- **Format**: Custom
- **Size**: ~400 abstracts
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Islamaj et al. (2019)
- **Notes**: Multi-type biomedical NER; chemical and disease mentions
- **URL**: <https://figshare.com/articles/dataset/MACCROBAT2018/9764942>

### ACE 2005 RE

**Rust ID**: `DatasetId::ACE05RE`

ACE 2005 relation extraction component. 7 entity types, 6 relation types with subtypes.

- **Language**: en
- **Domain**: news
- **Entity Types**: PER, ORG, GPE, LOC, FAC, VEH, WEA
- **Year**: 2005
- **Format**: XML
- **Size**: ~600 docs, 7 relation types
- **License**: LDC
- **Citation**: Walker et al. (2006)
- **Notes**: Classic RE benchmark; requires LDC license; often used with ACE NER
- **URL**: *Requires license or manual download*

### CoNLL04 RE

**Rust ID**: `DatasetId::CoNLL04RE`

Sentence-level relation extraction from CoNLL-2004. Clean, small RE benchmark.

- **Language**: en
- **Domain**: news
- **Entity Types**: PER, ORG, LOC, Other
- **Year**: 2004
- **Format**: CoNLL
- **Size**: ~1.4k sentences, 5 relation types
- **License**: Research
- **Citation**: Roth & Yih (2004)
- **Paper**: <https://aclanthology.org/W04-2401/>
- **Notes**: Clean sentence-level RE; joint NER+RE evaluation
- **URL**: <https://github.com/bekou/multihead_joint_entity_relation_extraction>

### CrossRE

**Rust ID**: `DatasetId::CrossRE`

Cross-domain relation extraction across 6 domains. Tests RE generalization.

- **Language**: en
- **Domain**: cross_domain
- **Entity Types**: PER, ORG, LOC, MISC
- **Year**: 2022
- **Format**: JSON
- **Size**: 6 domains: AI, Literature, Music, News, Politics, Science
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Bassignana & Plank (2022)
- **Paper**: <https://aclanthology.org/2022.emnlp-main.452/>
- **Notes**: Cross-domain RE evaluation; tests transfer across domains
- **URL**: <https://github.com/mainlp/CrossRE>

### UNER

**Rust ID**: `DatasetId::UNER`

Universal NER on Universal Dependencies. Gold NER with unified schema across 13 languages.

- **Language**: multi
- **Domain**: general
- **Entity Types**: PER, LOC, ORG
- **Year**: 2024
- **Format**: CoNLLU
- **Size**: 13 languages including Cebuano, Tagalog, Narabizi
- **License**: CC-BY-SA-4.0 (SPDX)
- **Citation**: Mayhew et al. (2024)
- **Paper**: <https://aclanthology.org/2024.naacl-long.243/>
- **Notes**: Unified NER on UD treebanks; includes low-resource languages; community-driven expansion
- **URL**: <https://github.com/UniversalNER/UNER>

### IndicNER

**Rust ID**: `DatasetId::IndicNER`

Indian languages NER covering 11 Indian languages. Low-resource multilingual NER.

- **Language**: multi
- **Domain**: general
- **Entity Types**: PER, LOC, ORG
- **Year**: 2022
- **Format**: CoNLL
- **Size**: 11 languages: Hindi, Bengali, Telugu, Tamil, Marathi, etc.
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Mhaske et al. (2022)
- **Paper**: <https://aclanthology.org/2022.findings-acl.269/>
- **Notes**: Indian language NER; part of AI4Bharat initiative; low-resource focus
- **URL**: <https://github.com/AI4Bharat/IndicNER>

### NorNE

**Rust ID**: `DatasetId::NorNE`

Norwegian NER covering Bokmål and Nynorsk. Morphologically rich language NER.

- **Language**: no
- **Domain**: general
- **Entity Types**: PER, LOC, ORG, GPE, PROD, EVT, DRV
- **Year**: 2020
- **Format**: CoNLL
- **Size**: ~600k tokens, both Bokmål and Nynorsk
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Jørgensen et al. (2020)
- **Paper**: <https://aclanthology.org/2020.lrec-1.559/>
- **Notes**: Both Norwegian written forms; morphologically rich; 8 entity types
- **URL**: <https://github.com/ltgoslo/norne>

### GermEval 2014

**Rust ID**: `DatasetId::GermEval2014`

German NER shared task. Standard German NER benchmark with nested entities.

- **Language**: de
- **Domain**: news
- **Entity Types**: PER, LOC, ORG, OTH
- **Year**: 2014
- **Format**: CoNLL
- **Annotation Scheme**: BIO
- **Size**: ~31k sentences
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Benikova et al. (2014)
- **Paper**: <https://aclanthology.org/W14-1707/>
- **Notes**: Standard German NER; includes nested/embedded entities; derived from Wikipedia and news
- **URL**: <https://sites.google.com/site/germaboreval2014/data>

### ReasoningNER

**Rust ID**: `DatasetId::ReasoningNER`

Zero-shot NER evaluation suite across 20 diverse datasets. Tests LLM NER capabilities.

- **Language**: en
- **Domain**: evaluation
- **Entity Types**: PER, LOC, ORG, MISC
- **Year**: 2025
- **Format**: JSONL
- **Size**: 20 datasets across news, social, biomedical, etc.
- **License**: Research
- **Citation**: Xia et al. (2025)
- **Paper**: <https://arxiv.org/abs/2511.11978>
- **Notes**: Zero-shot NER evaluation; tests instruction-following and entity reasoning in LLMs
- **URL**: <https://github.com/reasoning-ner/reasoning-ner>

### BioNER-LLaMA

**Rust ID**: `DatasetId::BioNERLLaMA`

Instruction-tuned biomedical NER benchmark. Evaluates generative models on disease/chemical/gene NER.

- **Language**: en
- **Domain**: biomedical
- **Entity Types**: Disease, Chemical, Gene
- **Year**: 2024
- **Format**: JSONL
- **Size**: Instruction-formatted from BC5CDR, NCBI, etc.
- **License**: Research
- **Citation**: Keloth et al. (2024)
- **Paper**: <https://academic.oup.com/bioinformatics/article/40/4/btae163/7633405>
- **Notes**: LLM instruction-tuning for BioNER; evaluates ChatGPT, LLaMA, etc. on biomedical entities
- **URL**: <https://github.com/BioNER-LLaMA/BioNER-LLaMA>

### Mention Resolution LLM

**Rust ID**: `DatasetId::MentionResolutionLLM`

MCQ-format coreference for LLMs from LitBank and FantasyCoref. Tests referential understanding on narratives.

- **Language**: en
- **Domain**: literature
- **Entity Types**: PER, LOC, ORG
- **Year**: 2024
- **Format**: JSONL
- **Size**: MCQ from LitBank + FantasyCoref
- **License**: Research
- **Citation**: Adams et al. (2024)
- **Paper**: <https://arxiv.org/abs/2411.07466>
- **Notes**: Multiple-choice coref for LLM evaluation; tests ambiguous, long-distance, nested mentions
- **URL**: <https://github.com/mention-resolution/mention-resolution-llm>

### ShARe 2013

**Rust ID**: `DatasetId::ShARe2013`

Clinical disorder mentions from ShARe/CLEF eHealth 2013. Discontinuous entity annotations.

- **Language**: en
- **Domain**: clinical
- **Entity Types**: DISORDER
- **Year**: 2013
- **Format**: Custom
- **Size**: ~300 clinical notes
- **License**: Research
- **Citation**: Pradhan et al. (2013)
- **Paper**: <https://aclanthology.org/S13-2056/>
- **Notes**: Clinical NER with discontinuous spans; shared task at CLEF eHealth
- **URL**: *Requires license or manual download*

### ShARe 2014

**Rust ID**: `DatasetId::ShARe2014`

Clinical disorder mentions from ShARe/CLEF eHealth 2014. Improved discontinuous NER annotations.

- **Language**: en
- **Domain**: clinical
- **Entity Types**: DISORDER, ANATOMY, MODIFIER
- **Year**: 2014
- **Format**: Custom
- **Size**: ~400 clinical notes
- **License**: Research
- **Citation**: Mowery et al. (2014)
- **Paper**: <https://aclanthology.org/S14-2007/>
- **Notes**: Improved clinical discontinuous NER; attribute normalization
- **URL**: *Requires license or manual download*

### i2b2 2010

**Rust ID**: `DatasetId::I2B2_2010`

Clinical concept extraction and assertion classification. Foundational clinical NER benchmark.

- **Language**: en
- **Domain**: clinical
- **Entity Types**: PROBLEM, TREATMENT, TEST
- **Year**: 2010
- **Format**: Custom
- **Size**: ~871 discharge summaries
- **License**: Research
- **Citation**: Uzuner et al. (2011)
- **Paper**: <https://academic.oup.com/jamia/article/18/5/552/833880>
- **Notes**: Foundational clinical NER; requires i2b2/n2c2 data use agreement
- **URL**: *Requires license or manual download*

### LexGLUE NER

**Rust ID**: `DatasetId::LexGLUENER`

Legal NER from LexGLUE benchmark. Legal entity extraction from case law and contracts.

- **Language**: en
- **Domain**: legal
- **Entity Types**: PERSON, ORGANIZATION, LOCATION, DATE, LEGAL_REF, COURT
- **Year**: 2022
- **Format**: JSONL
- **Size**: Part of LexGLUE benchmark suite
- **License**: Research
- **Citation**: Chalkidis et al. (2022)
- **Paper**: <https://aclanthology.org/2022.acl-long.297/>
- **Notes**: Legal domain benchmark; includes contracts, case law, legislation
- **URL**: <https://github.com/coastalcph/lex-glue>

### FinBen NER

**Rust ID**: `DatasetId::FinBenNER`

Financial NER from FinBen benchmark. Entity extraction from financial documents and filings.

- **Language**: en
- **Domain**: financial
- **Entity Types**: COMPANY, PERSON, MONEY, PERCENT, DATE, PRODUCT
- **Year**: 2024
- **Format**: JSONL
- **Size**: Multi-task financial benchmark
- **License**: Research
- **Citation**: Xie et al. (2024)
- **Paper**: <https://arxiv.org/abs/2402.12659>
- **Notes**: Financial IE benchmark; includes NER, classification, QA; 2024 NeurIPS
- **URL**: <https://github.com/TheFinAI/FinBen>

### FiNER-139

**Rust ID**: `DatasetId::FiNER139`

Financial NER with 139 fine-grained entity types. SEC 10-K/10-Q filings.

- **Language**: en
- **Domain**: financial
- **Entity Types**: COMPANY, EXECUTIVE, SUBSIDIARY, PRODUCT, REGULATION, FINANCIAL_METRIC
- **Year**: 2023
- **Format**: JSONL
- **Size**: ~10k sentences, 139 entity types
- **License**: MIT (SPDX)
- **Citation**: Shah et al. (2023)
- **Notes**: Fine-grained financial NER; hierarchical entity types; SEC filings
- **URL**: <https://github.com/FiNER-139/FiNER-139>

### taggedPBC Esperanto

**Rust ID**: `DatasetId::TaggedPBCEsperanto`

POS-tagged Esperanto from Parallel Bible Corpus. ~1800 sentences with word-level alignment.

- **Language**: eo
- **Domain**: religious
- **Entity Types**: PER, LOC, ORG
- **Year**: 2024
- **Format**: CoNLLU
- **Size**: ~1800 sentences, New Testament
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Zeman et al. (2025)
- **Paper**: <https://arxiv.org/abs/2505.12560>
- **Notes**: First large-scale annotated Esperanto corpus; cross-linguistic POS; no dedicated NER layer yet
- **URL**: <https://github.com/clab/taggedPBC>

### taggedPBC Klingon

**Rust ID**: `DatasetId::TaggedPBCKlingon`

POS-tagged Klingon from Parallel Bible Corpus. OVS word order with complex verbal morphology.

- **Language**: tlh
- **Domain**: religious
- **Entity Types**: PER, LOC, ORG
- **Year**: 2024
- **Format**: CoNLLU
- **Size**: ~1800 sentences, New Testament
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Zeman et al. (2025)
- **Paper**: <https://arxiv.org/abs/2505.12560>
- **Notes**: Klingon has OVS word order, agglutinative verbs with suffix slots; tests non-SVO processing
- **URL**: <https://github.com/clab/taggedPBC>

### UD Esperanto Cairo

**Rust ID**: `DatasetId::UDEsperantoCairo`

Universal Dependencies treebank for Esperanto. Syntax annotation without NER layer.

- **Language**: eo
- **Domain**: general
- **Entity Types**: PER, LOC, ORG
- **Year**: 2020
- **Format**: CoNLLU
- **Size**: 2 documents (Manifesto, Cairo sample)
- **License**: CC-BY-SA-4.0 (SPDX)
- **Citation**: Wennerberg (2020)
- **Paper**: <https://universaldependencies.org/eo/index.html>
- **Notes**: Small treebank illustrating UD annotation for Esperanto; no NER layer but suitable base for annotation
- **URL**: <https://raw.githubusercontent.com/UniversalDependencies/UD_Esperanto-Cairo/master/eo_cairo-ud-test.conllu>

### Klingon Effect LID

**Rust ID**: `DatasetId::KlingonEffectLID`

Language ID dataset with 11 constructed languages. 14.2M sentences across 101 languages.

- **Language**: multi
- **Domain**: general
- **Year**: 2025
- **Format**: Custom
- **Size**: 14.2M sentences, 101 languages (11 constructed)
- **License**: Research
- **Citation**: Moura et al. (2025)
- **Paper**: <https://wmdqs.org/submissions-2025/19.pdf>
- **Notes**: Shows constructed languages (Esperanto, Klingon, Ido, Interlingua) outperform natural languages in LID
- **URL**: <https://wmdqs.org/submissions-2025/19.pdf>

### Lojban Tatoeba

**Rust ID**: `DatasetId::LojbanTatoeba`

Lojban-English sentence pairs from Tatoeba. Logical language translation corpus.

- **Language**: jbo
- **Domain**: general
- **Year**: 2024
- **Format**: TSV
- **Size**: ~3k sentence pairs
- **License**: CC-BY-2.0 (SPDX)
- **Citation**: Tatoeba Project (2024)
- **Notes**: Logical constructed language; predicate logic syntax; useful for semantic parsing studies
- **URL**: <https://tatoeba.org/en/downloads>

### Interlingue Wikipedia

**Rust ID**: `DatasetId::InterlingueWikipedia`

Interlingue (Occidental) Wikipedia text corpus. International auxiliary language.

- **Language**: ie
- **Domain**: encyclopedia
- **Year**: 2024
- **Format**: XML
- **Size**: ~4k articles
- **License**: CC-BY-SA-4.0 (SPDX)
- **Citation**: Wikimedia (2024)
- **Notes**: Western European vocabulary roots; naturalistic IAL; smaller than Esperanto Wikipedia
- **URL**: <https://dumps.wikimedia.org/iewiki/>

### Toki Pona Corpus

**Rust ID**: `DatasetId::TokiPonaCorpus`

Toki Pona minimalist language corpus. 120-word language for semantic simplification.

- **Language**: tok
- **Domain**: general
- **Year**: 2021
- **Format**: TXT
- **Size**: ~50k tokens
- **License**: CC0-1.0
- **Citation**: Lang (2021)
- **Notes**: Philosophical constructed language; only 120 words; tests compositional semantics
- **URL**: <https://github.com/kilipan/toki-pona-corpus>

### OmniNER2025

**Rust ID**: `DatasetId::OmniNER2025`

Diverse fine-grained Chinese NER covering informal text (social media, forums). Large-scale benchmark for modern NER models.

- **Language**: zh
- **Domain**: social_media
- **Entity Types**: PER, LOC, ORG, GPE, FAC, PRODUCT, EVENT
- **Year**: 2025
- **Format**: JSONL
- **Size**: Large-scale Chinese informal text
- **License**: Research
- **Citation**: OmniNER Team (2025)
- **Paper**: <https://dl.acm.org/doi/10.1145/3726302.3730048>
- **Notes**: 2025 benchmark for fine-grained Chinese NER; expands beyond formal text; tests LLM capabilities
- **URL**: *Requires license or manual download*

### LegalCore

**Rust ID**: `DatasetId::LegalCore`

Event coreference in long legal documents. Long-distance cross-section event links.

- **Language**: en
- **Domain**: legal
- **Entity Types**: EVENT, PARTICIPANT, TIME
- **Year**: 2025
- **Format**: JSONL
- **Size**: Long legal documents, largest tokens per document
- **License**: Research
- **Citation**: ACL Findings (2025)
- **Paper**: <https://aclanthology.org/2025.findings-acl.1284.pdf>
- **Notes**: ACL 2025; benchmarks Llama-3.1, Mistral, Qwen, GPT-4; LLMs underperform supervised baselines
- **URL**: *Requires license or manual download*

### Z-coref

**Rust ID**: `DatasetId::Zcoref`

Joint coreference and zero-pronoun resolution. For languages with pro-drop (Chinese, Japanese, Korean).

- **Language**: multi
- **Domain**: general
- **Entity Types**: ZERO_PRONOUN, ENTITY
- **Year**: 2024
- **Format**: CoNLL
- **Size**: Multi-language pro-drop coreference
- **License**: Research
- **Citation**: Z-coref Authors (2024)
- **Paper**: <https://arxiv.org/pdf/2504.05824>
- **Notes**: Tests handling of dropped arguments; critical for CJK languages; zero anaphora resolution
- **URL**: *Requires license or manual download*

### MHERCL

**Rust ID**: `DatasetId::MHERCL`

Historical long-tail entity linking benchmark. Tests LLM behavior on rare/historical Wikidata entities.

- **Language**: en
- **Domain**: historical
- **Entity Types**: HISTORICAL_ENTITY
- **Year**: 2025
- **Format**: JSONL
- **Size**: Long-tail historical entities
- **License**: Research
- **Citation**: MHERCL Authors (2025)
- **Paper**: <https://arxiv.org/html/2505.03473v1>
- **Notes**: v0.1; tests EL on niche historical entities; analyzes LLM behavior on rare entities
- **URL**: <https://arxiv.org/html/2505.03473v1>

### SNOMED CT EL Challenge

**Rust ID**: `DatasetId::SNOMEDChallenge`

Clinical entity linking to SNOMED CT. From SNOMED International 2024 challenge.

- **Language**: en
- **Domain**: clinical
- **Entity Types**: CLINICAL_CONCEPT
- **Year**: 2024
- **Format**: Custom
- **Size**: Clinical notes, SNOMED CT linked
- **License**: Research
- **Citation**: SNOMED International (2024)
- **Paper**: <https://www.snomed.org/news/snomed-international-announces-entity-linking-challenge-winners>
- **Notes**: 2024 challenge dataset; SNOMED CT coded clinical text; benchmarks clinical EL systems
- **URL**: *Requires license or manual download*

### ESCO Skills EL

**Rust ID**: `DatasetId::ESCOSkillsEL`

Entity linking for occupational skills to ESCO taxonomy. Job market domain, multilingual.

- **Language**: multi
- **Domain**: general
- **Entity Types**: SKILL
- **Year**: 2024
- **Format**: Custom
- **Size**: Skill mentions across multiple languages
- **License**: Research
- **Citation**: EACL Findings (2024)
- **Paper**: <https://aclanthology.org/2024.findings-eacl.28/>
- **Notes**: Complements MELO; links skills (not occupations) to ESCO taxonomy; job posting text
- **URL**: *Requires license or manual download*

### NatureLM-audio

**Rust ID**: `DatasetId::NatureLMAudio`

Foundation model training collection for bioacoustics. Multi-species audio-text pairs.

- **Language**: en
- **Domain**: bioacoustics
- **Entity Types**: SPECIES, CALL_TYPE, BEHAVIOR
- **Year**: 2024
- **Format**: Custom
- **Size**: Multi-taxon audio-text pairs (birds, marine mammals, primates)
- **License**: Research
- **Citation**: NatureLM Team (2024)
- **Paper**: <https://arxiv.org/abs/2411.07186>
- **Notes**: Bioacoustic foundation model data; paired audio-text descriptions; cross-taxa experiments
- **URL**: <https://github.com/earthspecies/naturelm-audio>

### BEANS-Zero

**Rust ID**: `DatasetId::BEANSZero`

Bioacoustics benchmark beyond species classification. Natural-language prompts for animal sounds.

- **Language**: en
- **Domain**: bioacoustics
- **Entity Types**: SPECIES, CALL_TYPE, INDIVIDUAL
- **Year**: 2024
- **Format**: Custom
- **License**: Research
- **Citation**: NatureLM Team (2024)
- **Paper**: <https://arxiv.org/abs/2411.07186>
- **Notes**: Zero-shot transfer to unseen taxa; captioning, retrieval, instruction-following on animal vocalizations
- **URL**: <https://github.com/earthspecies/beans-zero>

### NLM-Chem

**Rust ID**: `DatasetId::NLMChem`

Chemical entity recognition and normalization. Full-text PMC articles with MeSH identifiers.

- **Language**: en
- **Domain**: biomedical
- **Entity Types**: CHEMICAL, DRUG
- **Year**: 2021
- **Format**: BRAT
- **Size**: ~150 full-text articles, ~38k annotations
- **License**: Public (SPDX)
- **Citation**: Islamaj et al. (2021)
- **Paper**: <https://academic.oup.com/database/article/doi/10.1093/database/baac102/6858529>
- **Notes**: Gold-standard chemical NER; normalized to MeSH; used for BioCreative VII
- **URL**: <https://ftp.ncbi.nlm.nih.gov/pub/lu/NLM-Chem/>

### CHEMDNER

**Rust ID**: `DatasetId::CHEMDNER`

Chemical compound and drug name recognition in scientific text.

- **Language**: en
- **Domain**: biomedical
- **Entity Types**: CHEMICAL, DRUG, ABBREVIATION
- **Year**: 2015
- **Format**: BIO
- **Size**: ~10k abstracts
- **License**: Research
- **Citation**: Krallinger et al. (2015)
- **Paper**: <https://jcheminf.biomedcentral.com/articles/10.1186/1758-2946-7-S1-S2>
- **Notes**: BioCreative IV shared task; abstract-level chemical NER; foundational chemistry benchmark
- **URL**: <https://biocreative.bioinformatics.udel.edu/tasks/biocreative-iv/chemdner/>

### TimeBank-Dense

**Rust ID**: `DatasetId::TimeBankDense`

Dense temporal relation annotation. Re-annotation of TimeBank with more consistent TLINK labels.

- **Language**: en
- **Domain**: news
- **Entity Types**: EVENT, TIMEX3
- **Year**: 2014
- **Format**: TimeML
- **Size**: ~36 documents, dense annotation
- **License**: Research
- **Citation**: Chambers et al. (2014)
- **Paper**: <https://aclanthology.org/Q14-1002/>
- **Notes**: Event-event temporal relations; BEFORE/AFTER/INCLUDES/VAGUE; timeline construction benchmark
- **URL**: <https://github.com/bethard/timebank-dense>

### Twitter-GMNER

**Rust ID**: `DatasetId::TwitterGMNER`

Grounded Multimodal NER. Entities linked to bounding boxes in social media images.

- **Language**: en
- **Domain**: social_media
- **Entity Types**: PER, LOC, ORG, MISC
- **Year**: 2024
- **Format**: JSONL
- **Size**: ~8k tweets with images
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Li et al. (2024)
- **Paper**: <https://aclanthology.org/2024.findings-acl.58/>
- **Notes**: Entity mentions grounded to image regions; visual-textual entity alignment
- **URL**: <https://github.com/JinYuanLi0012/RiVEG>

### MNER-MI

**Rust ID**: `DatasetId::MNERMI`

Multimodal NER with Multiple Images. Social media posts with multiple image context.

- **Language**: en
- **Domain**: social_media
- **Entity Types**: PER, LOC, ORG, MISC
- **Year**: 2024
- **Format**: JSONL
- **Size**: ~5k tweets with multiple images
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Wang et al. (2024)
- **Paper**: <https://aclanthology.org/2024.lrec-main.1001/>
- **Notes**: Multi-image context improves NER; temporal-prompt model baseline; LREC-COLING 2024
- **URL**: <https://github.com/NUSTM/MNER-MI>

### 2M-NER

**Rust ID**: `DatasetId::TwoMNER`

Multilingual Multimodal NER. Four languages with text-image pairs.

- **Language**: multi
- **Domain**: social_media
- **Entity Types**: PER, LOC, ORG, MISC
- **Year**: 2024
- **Format**: JSONL
- **Size**: ~20k examples, 4 languages (EN, FR, DE, ES)
- **License**: Apache-2.0 (SPDX)
- **Citation**: Liu et al. (2024)
- **Paper**: <https://arxiv.org/abs/2404.17122>
- **Notes**: Contrastive text-image alignment; multilingual multimodal NER benchmark
- **URL**: <https://github.com/Alibaba-NLP/2M-NER>

### Mathematical Entities

**Rust ID**: `DatasetId::MathEntities`

Terminology and definition extraction from mathematical text. Category theory corpora.

- **Language**: en
- **Domain**: scientific
- **Entity Types**: TERM, DEFINITION, THEOREM
- **Year**: 2024
- **Format**: LaTeX
- **Size**: ~3 corpora in category theory
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Mazzei et al. (2024)
- **Paper**: <https://aclanthology.org/2024.lrec-main.966/>
- **Notes**: LaTeX source preservation; math-aware NER; entity linking to Wikidata/nLab
- **URL**: <https://github.com/dmazzei/mathematical-entities>

### SciERC

**Rust ID**: `DatasetId::SciERC`

Scientific information extraction from AI/ML papers. Nested entities and relations.

- **Language**: en
- **Domain**: scientific
- **Entity Types**: TASK, METHOD, METRIC, MATERIAL, GENERIC, OTHER
- **Year**: 2018
- **Format**: JSONL
- **Size**: ~500 abstracts
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Luan et al. (2018)
- **Paper**: <https://aclanthology.org/D18-1360/>
- **Notes**: Canonical scientific NER + relation extraction; nested entities common
- **URL**: <https://nlp.cs.washington.edu/sciIE/>

### GeoWebNews

**Rust ID**: `DatasetId::GeoWebNews`

Geoparsing benchmark from web news. Toponyms with geocoding coordinates.

- **Language**: en
- **Domain**: news
- **Entity Types**: LOC, GPE, FACILITY
- **Year**: 2020
- **Format**: CoNLL
- **Size**: ~4k documents
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Gritta et al. (2020)
- **Paper**: <https://aclanthology.org/2020.lrec-1.381/>
- **Notes**: Toponym recognition + resolution; GeoNames linking; web news geoparsing
- **URL**: <https://github.com/milangritta/GeoWebNews>

### LGL

**Rust ID**: `DatasetId::LGL`

Local-Global Lexicon for toponym disambiguation. News articles with geolocation.

- **Language**: en
- **Domain**: news
- **Entity Types**: LOC
- **Year**: 2010
- **Format**: Custom
- **Size**: ~5.8k place references
- **License**: MIT (SPDX)
- **Citation**: Lieberman et al. (2010)
- **Notes**: Toponym disambiguation benchmark; local vs global context for geolocation
- **URL**: <https://github.com/wikipedia2vec/wikipedia2vec>

### TASTEset

**Rust ID**: `DatasetId::TASTEset`

Recipe ingredient NER. 700 annotated recipe ingredient lists with 9 entity classes.

- **Language**: en
- **Domain**: food
- **Entity Types**: INGREDIENT, QUANTITY, UNIT, STATE, SIZE, TEMP
- **Year**: 2023
- **Format**: BIO
- **Size**: ~700 ingredient lists
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: TASTEset Team (2023)
- **Notes**: Recipe NER benchmark; BIO/BILOU conversion utilities; BERT model pipeline
- **URL**: <https://github.com/taisti/TASTEset>

### Recipe NER

**Rust ID**: `DatasetId::RecipeNER`

Deep learning recipe NER. Multi-scale datasets with ingredient and instruction entities.

- **Language**: en
- **Domain**: food
- **Entity Types**: INGREDIENT, QUANTITY, UNIT, PROCESS, UTENSIL, TEMP
- **Year**: 2024
- **Format**: BIO
- **Size**: ~88k phrases (6.6k manual, 26k augmented, 88k machine)
- **License**: MIT (SPDX)
- **Citation**: Deepgram (2024)
- **Paper**: <https://aclanthology.org/2024.lrec-main.406/>
- **Notes**: Three-tier dataset; spaCy-transformer achieves 96% F1; recipe IE pipeline
- **URL**: <https://github.com/cosylabiiit/recipe-ner>

### CodeSearchNet

**Rust ID**: `DatasetId::CodeSearchNet`

Code understanding benchmark. Function documentation and code search across 6 languages.

- **Language**: multi
- **Domain**: code
- **Entity Types**: FUNCTION, CLASS, VARIABLE, MODULE
- **Year**: 2019
- **Format**: JSONL
- **Size**: ~2M functions across 6 programming languages
- **License**: MIT (SPDX)
- **Citation**: Husain et al. (2019)
- **Paper**: <https://arxiv.org/abs/1909.09436>
- **Notes**: Code-docstring pairs; Python, Java, Go, PHP, JavaScript, Ruby; foundation for code NER
- **URL**: <https://github.com/github/CodeSearchNet>

### FABLE

**Rust ID**: `DatasetId::FABLE`

Fiction Adapted BERT for Literary Entities. DeBERTa-based NER for narrative fiction.

- **Language**: en
- **Domain**: fiction
- **Entity Types**: CHARACTER, LOCATION, ORGANIZATION, ARTIFACT
- **Year**: 2024
- **Format**: Custom
- **License**: MIT (SPDX)
- **Citation**: FABLE Team (2024)
- **Notes**: Literary NER model; targets invented names in fantasy/SF; trained on narrative fiction
- **URL**: <https://huggingface.co/DeBERTa-literary-entities>

### ELGold

**Rust ID**: `DatasetId::ELGold`

Gold-standard multi-genre Polish NER+EL. Includes fiction, press, blogs.

- **Language**: pl
- **Domain**: general
- **Entity Types**: PER, LOC, ORG, MISC
- **Year**: 2025
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Pokrywka et al. (2025)
- **Paper**: <https://www.nature.com/articles/s41597-025-05274-4>
- **Notes**: Multi-genre including fiction; Wikipedia-linked; Polish language
- **URL**: <https://mostwiedzy.pl/en/open-research-data/elgold-gold-standard-multi-genre-dataset>

### Streaming CD-Coref

**Rust ID**: `DatasetId::StreamingCDCoref`

Streaming cross-document entity coreference protocol. News domain streaming evaluation.

- **Language**: en
- **Domain**: news
- **Entity Types**: PER, ORG, LOC
- **Year**: 2010
- **Format**: Custom
- **License**: Research
- **Citation**: Dredze et al. (2010)
- **Paper**: <https://aclanthology.org/C10-1032/>
- **Notes**: Canonical streaming entity clustering; O(n) single-pass; evolving cluster representations
- **URL**: <https://www.cs.jhu.edu/~mdredze/publications/streaming_coref_coling.pdf>

### Tem-DocRED

**Rust ID**: `DatasetId::TemDocRED`

Temporal document-level relation extraction. Converts static triples to temporal quadruples.

- **Language**: en
- **Domain**: wikipedia
- **Entity Types**: PER, ORG, LOC, TIME
- **Year**: 2024
- **Format**: JSONL
- **Size**: Re-DocRED + temporal timestamps
- **License**: MIT (SPDX)
- **Citation**: Zhang et al. (2024)
- **Paper**: <https://pmc.ncbi.nlm.nih.gov/articles/PMC12048500/>
- **Notes**: Temporal KG construction from documents; LLM + pattern mining for timestamp inference
- **URL**: <https://github.com/THUDM/Tem-DocRED>

### SciCo-Radar

**Rust ID**: `DatasetId::SciCoRadar`

Scientific cross-document concept coreference. Dynamic definitions via LLM retrieval.

- **Language**: en
- **Domain**: scientific
- **Entity Types**: CONCEPT, METHOD, TASK, MATERIAL
- **Year**: 2024
- **Format**: JSONL
- **License**: Apache-2.0 (SPDX)
- **Citation**: Wadden et al. (2024)
- **Paper**: <https://arxiv.org/abs/2409.15113>
- **Notes**: Cross-doc concept coref with hierarchy; LLM-generated relational definitions improve F1
- **URL**: <https://github.com/allenai/scico-radar>

### Event KG Drift

**Rust ID**: `DatasetId::EventKGDrift`

Multi-perspective concept drift detection on event knowledge graphs.

- **Language**: en
- **Domain**: evaluation
- **Entity Types**: EVENT, CASE, ACTOR, TIME
- **Year**: 2024
- **Format**: Custom
- **License**: Research
- **Citation**: TU Eindhoven (2024)
- **Notes**: Actor-centric features give 2.6x stronger drift signals; temporal graph drift on EKGs
- **URL**: <https://research.tue.nl/files/349781334/978-3-031-61057-8_9.pdf>

### Wikidata Semantic Drift

**Rust ID**: `DatasetId::WikidataDrift`

Semantic drift detection in Wikidata. LLM-based classification inconsistency detection.

- **Language**: multi
- **Domain**: encyclopedia
- **Year**: 2024
- **Format**: Custom
- **License**: CC0-1.0
- **Citation**: Wikidata Drift Team (2024)
- **Paper**: <https://arxiv.org/abs/2511.04926>
- **Notes**: Multi-dimensional semantic risk model; drift threshold ~0.6; continuous KG curation
- **URL**: <https://arxiv.org/abs/2511.04926>

### AIDA-CoNLL (v2)

**Rust ID**: `DatasetId::AIDA`

Entity linking to Wikipedia. CoNLL-YAGO dataset for named entity disambiguation.

- **Language**: en
- **Domain**: news
- **Entity Types**: PER, LOC, ORG, MISC
- **Year**: 2011
- **Format**: CoNLL
- **License**: Research
- **Citation**: Hoffart et al. (2011)
- **Paper**: <https://aclanthology.org/D11-1072/>
- **Notes**: Entity linking benchmark; links CoNLL-2003 mentions to YAGO/Wikipedia
- **URL**: <https://www.mpi-inf.mpg.de/departments/databases-and-information-systems/research/ambiverse-nlu/aida/downloads>

### AIONER

**Rust ID**: `DatasetId::AIONER`

All-in-one biomedical NER. Unified biomedical entity extraction model.

- **Language**: en
- **Domain**: biomedical
- **Entity Types**: Gene, Disease, Chemical, Species
- **Year**: 2023
- **Format**: JSONL
- **License**: Research
- **Citation**: Luo et al. (2023)
- **Notes**: Unified model for multiple biomedical entity types
- **URL**: <https://github.com/AIONER/AIONER>

### AISHELL-NER

**Rust ID**: `DatasetId::AISHELLNER`

Chinese speech NER from AISHELL corpus. Named entities in Mandarin speech.

- **Language**: zh
- **Domain**: speech
- **Entity Types**: PER, LOC, ORG
- **Year**: 2017
- **Format**: Custom
- **License**: Research
- **Citation**: AISHELL Foundation (2017)
- **Notes**: Speech transcription NER; tests robustness to ASR errors
- **URL**: <https://www.aishelltech.com/aishell_2>

### AstroNER

**Rust ID**: `DatasetId::AstroNER`

Astronomy named entity recognition. Celestial objects and astronomical concepts.

- **Language**: en
- **Domain**: astrophysics
- **Entity Types**: CelestialObject, Instrument, Mission, Phenomenon
- **Year**: 2022
- **Format**: CoNLL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: NASA ADS Team
- **Notes**: Domain-specific NER for astronomy literature
- **URL**: <https://github.com/astronomical-ner/AstroNER>

### B2NERD

**Rust ID**: `DatasetId::B2NERD`

Billion-scale news NER dataset. Large-scale distantly supervised NER.

- **Language**: en
- **Domain**: news
- **Entity Types**: PER, LOC, ORG, MISC
- **Year**: 2023
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Umean (2023)
- **Notes**: Large-scale silver-standard NER; useful for pre-training
- **URL**: <https://huggingface.co/datasets/Umean/B2NERD>

### BioMNER

**Rust ID**: `DatasetId::BioMNER`

Biomedical method NER. Scientific methods and techniques in biomedical text.

- **Language**: en
- **Domain**: biomedical
- **Entity Types**: Method, Technique, Protocol
- **Year**: 2004
- **Format**: BIO
- **License**: Research
- **Citation**: BioNLP (2004)
- **Notes**: Biomedical methodology extraction; from BioNLP shared task
- **URL**: <https://huggingface.co/datasets/tner/bionlp2004>

### LegNER

**Rust ID**: `DatasetId::LegNER`

Legal domain NER. Named entities in legal documents and court opinions.

- **Language**: en
- **Domain**: legal
- **Entity Types**: Court, Judge, Statute, Party, Date
- **Year**: 2021
- **Format**: CoNLL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Legal NLP Team (2021)
- **Notes**: Legal domain specialization; court documents and statutes
- **URL**: <https://github.com/Liquid-Legal-Institute/LegalBench>

### OpenNER 1.0

**Rust ID**: `DatasetId::OpenNER`

Open domain NER benchmark. Broad coverage across multiple domains.

- **Language**: en
- **Domain**: mixed
- **Entity Types**: PER, LOC, ORG, EVENT, PRODUCT
- **Year**: 2023
- **Format**: JSONL
- **License**: CC-BY-SA-4.0 (SPDX)
- **Citation**: Babelscape (2023)
- **Notes**: Open-domain NER; tests generalization across domains
- **URL**: <https://huggingface.co/datasets/Babelscape/OpenNER>

### SciNER

**Rust ID**: `DatasetId::SciNER`

Scientific literature NER. Entities from scientific papers across disciplines.

- **Language**: en
- **Domain**: scientific
- **Entity Types**: Method, Task, Dataset, Metric, Material
- **Year**: 2022
- **Format**: JSONL
- **License**: Apache-2.0 (SPDX)
- **Citation**: Allen AI (2022)
- **Notes**: Scientific entities; paper abstracts and methods sections
- **URL**: <https://github.com/allenai/sciner>

### FinanceNER

**Rust ID**: `DatasetId::FinanceNER`

Financial domain NER. Named entities from financial documents and news.

- **Language**: en
- **Domain**: financial
- **Entity Types**: Company, Stock, Currency, Amount, Date
- **Year**: 2020
- **Format**: CoNLL
- **License**: Research
- **Citation**: FinNLP (2020)
- **Notes**: Financial entity extraction; SEC filings and news
- **URL**: <https://github.com/nlpaueb/finer>

### TechNER

**Rust ID**: `DatasetId::TechNER`

Technology domain NER. Software, hardware, and technical entities.

- **Language**: en
- **Domain**: code
- **Entity Types**: Software, Hardware, Company, Version, Language
- **Year**: 2021
- **Format**: CoNLL
- **License**: MIT (SPDX)
- **Citation**: TechNER Team (2021)
- **Notes**: Technology entities; Stack Overflow and documentation
- **URL**: <https://github.com/techner/techner>

### FictionNER-750M

**Rust ID**: `DatasetId::FictionNER750M`

Fiction NER at scale. Named entities from 750M tokens of fiction text.

- **Language**: en
- **Domain**: fiction
- **Entity Types**: Character, Location, Object, Organization
- **Year**: 2023
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Fiction NER Team (2023)
- **Notes**: Large-scale fiction NER; novels and short stories
- **URL**: <https://huggingface.co/datasets/fiction-ner/750M>

### Character Codex

**Rust ID**: `DatasetId::CharacterCodex`

Character entity recognition in fiction. Literary character identification.

- **Language**: en
- **Domain**: fiction
- **Entity Types**: Character, Alias, Role
- **Year**: 2022
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Character Codex Team (2022)
- **Notes**: Character tracking across narrative; aliases and roles
- **URL**: <https://github.com/character-codex/character-codex>

### MUC-6

**Rust ID**: `DatasetId::MUC6`

Message Understanding Conference 6. Seminal NER and coreference dataset.

- **Language**: en
- **Domain**: news
- **Entity Types**: ENAMEX, TIMEX, NUMEX
- **Year**: 1996
- **Format**: SGML
- **License**: LDC
- **Citation**: Grishman & Sundheim (1996)
- **Paper**: <https://aclanthology.org/C96-1079/>
- **Notes**: Historically significant; established NER evaluation paradigm
- **URL**: <https://catalog.ldc.upenn.edu/LDC2003T13>

### MUC-7

**Rust ID**: `DatasetId::MUC7`

Message Understanding Conference 7. Expanded NE types from MUC-6.

- **Language**: en
- **Domain**: news
- **Entity Types**: ENAMEX, TIMEX, NUMEX
- **Year**: 1998
- **Format**: SGML
- **License**: LDC
- **Citation**: Chinchor (1998)
- **Paper**: <https://aclanthology.org/M98-1002/>
- **Notes**: Refined MUC-6 guidelines; includes satellite launch texts
- **URL**: <https://catalog.ldc.upenn.edu/LDC2001T02>

### OntoNotes 5.0

**Rust ID**: `DatasetId::OntoNotes50`

OntoNotes Release 5.0. Multi-genre corpus with NER, coref, and more.

- **Language**: en
- **Domain**: mixed
- **Entity Types**: PER, ORG, GPE, LOC, FAC, NORP, EVENT, WORK_OF_ART, LAW, LANGUAGE, DATE, TIME, PERCENT, MONEY, QUANTITY, ORDINAL, CARDINAL
- **Year**: 2013
- **Format**: CoNLL
- **Annotation Scheme**: CoNLLCoref
- **Size**: ~2.9M words across genres
- **License**: LDC
- **Citation**: Weischedel et al. (2013)
- **Paper**: <https://catalog.ldc.upenn.edu/docs/LDC2013T19/OntoNotes-Release-5.0.pdf>
- **Notes**: Gold standard for multiple NLP tasks; WSJ, broadcast, web, telephone
- **URL**: <https://catalog.ldc.upenn.edu/LDC2013T19>

### GUM

**Rust ID**: `DatasetId::GUM`

Georgetown University Multilayer corpus. Rich annotation across 12 genres.

- **Language**: en
- **Domain**: mixed
- **Entity Types**: person, place, organization, time, event
- **Year**: 2017
- **Format**: CoNLL
- **Size**: ~200k tokens, 12 genres
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Zeldes (2017)
- **Paper**: <https://aclanthology.org/W17-0809/>
- **Notes**: Multi-layer annotation; coreference, RST, entities
- **URL**: <https://github.com/amir-zeldes/gum>

### TAC-KBP

**Rust ID**: `DatasetId::TACKBP`

TAC Knowledge Base Population. Entity linking and slot filling benchmark.

- **Language**: en
- **Domain**: news
- **Entity Types**: PER, ORG, GPE
- **Year**: 2010
- **Format**: Custom
- **License**: LDC
- **Citation**: Ji et al. (2010)
- **Paper**: <https://aclanthology.org/C10-1058/>
- **Notes**: Entity linking to Wikipedia/KB; slot filling for attributes
- **URL**: <https://tac.nist.gov/>

### HAREM

**Rust ID**: `DatasetId::HAREM`

Portuguese NER evaluation. First and Second HAREM conferences.

- **Language**: pt
- **Domain**: news
- **Entity Types**: PESSOA, LOCAL, ORGANIZACAO, TEMPO, VALOR
- **Year**: 2006
- **Format**: SGML
- **License**: Research
- **Citation**: Santos et al. (2006)
- **Paper**: <https://www.linguateca.pt/HAREM/>
- **Notes**: Portuguese NER benchmark; morphologically rich language
- **URL**: <https://www.linguateca.pt/HAREM/>

### Gun Violence Corpus (v2)

**Rust ID**: `DatasetId::GunViolenceCorpus`

Gun violence event extraction. Named entities and events from news.

- **Language**: en
- **Domain**: news
- **Entity Types**: Shooter, Victim, Weapon, Location, Date
- **Year**: 2016
- **Format**: Custom
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Pavlick et al. (2016)
- **Notes**: Event extraction; sensitive domain requiring careful handling
- **URL**: <https://github.com/gun-violence-corpus/gvc>

### SLUE

**Rust ID**: `DatasetId::SLUE`

Spoken Language Understanding Evaluation. NER in speech transcripts.

- **Language**: en
- **Domain**: speech
- **Entity Types**: PER, LOC, ORG, DATE
- **Year**: 2022
- **Format**: JSONL
- **License**: MIT (SPDX)
- **Citation**: Shon et al. (2022)
- **Paper**: <https://aclanthology.org/2022.naacl-main.137/>
- **Notes**: End-to-end speech NER; VoxPopuli and VoxCeleb sources
- **URL**: <https://github.com/asappresearch/slue-toolkit>

### CRAFT Coreference

**Rust ID**: `DatasetId::CRAFTCoref`

Colorado Richly Annotated Full-Text corpus coreference. Biomedical coref.

- **Language**: en
- **Domain**: biomedical
- **Entity Types**: Gene, Protein, Cell, Organism
- **Year**: 2017
- **Format**: Standoff
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Cohen et al. (2017)
- **Paper**: <https://academic.oup.com/database/article/doi/10.1093/database/bax087/4621360>
- **Notes**: Full-text biomedical articles; coreference including bridging
- **URL**: <https://github.com/UCDenver-ccp/CRAFT>

### Football Coreference Corpus (v2)

**Rust ID**: `DatasetId::FootballCorefCorpus`

Cross-document event coreference for football matches.

- **Language**: en
- **Domain**: sports
- **Entity Types**: Event, Team, Player, Location
- **Year**: 2018
- **Format**: Custom
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Vossen et al. (2018)
- **Notes**: Cross-document event coreference; sports domain
- **URL**: <https://github.com/cltl/FCC>

### Multiparty Dialogue Coreference

**Rust ID**: `DatasetId::MultipartyDialogueCoref`

Coreference in multi-party conversations. Meeting and chat transcripts.

- **Language**: en
- **Domain**: dialogue
- **Entity Types**: PER, ORG, LOC
- **Year**: 2020
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Sarkar et al. (2020)
- **Notes**: Multi-party setting; speaker identification challenges
- **URL**: <https://github.com/sopan-sarkar/multiparty-dialogue-coref>

### CODI-CRAC

**Rust ID**: `DatasetId::CODICRAC`

CODI/CRAC shared task on anaphora and coreference. Multiple languages.

- **Language**: multi
- **Domain**: mixed
- **Entity Types**: PER, ORG, LOC, Event
- **Year**: 2022
- **Format**: CoNLL
- **Annotation Scheme**: CoNLLCoref
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: CODI-CRAC Team (2022)
- **Paper**: <https://aclanthology.org/2022.codi-1.0/>
- **Notes**: Shared task data; includes bridging and discourse deixis
- **URL**: <https://github.com/UniversalAnaphora/UA-CODI-CRAC>

### MixRED

**Rust ID**: `DatasetId::MixRED`

Mixed relation extraction dataset. Multiple relation types and domains.

- **Language**: en
- **Domain**: mixed
- **Entity Types**: PER, ORG, LOC
- **Year**: 2022
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: MixRED Team (2022)
- **Notes**: Relation extraction across multiple domains
- **URL**: <https://github.com/mixred/MixRED>

### CovEReD

**Rust ID**: `DatasetId::CovEReD`

COVID-19 relation extraction dataset. Biomedical relations from pandemic literature.

- **Language**: en
- **Domain**: biomedical
- **Entity Types**: Drug, Disease, Gene, Symptom
- **Year**: 2021
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: CovEReD Team (2021)
- **Notes**: COVID-19 specific; drug-disease-gene relations
- **URL**: <https://github.com/covered/CovEReD>

### SciER

**Rust ID**: `DatasetId::SciER`

Scientific entity and relation extraction. From AI/ML papers.

- **Language**: en
- **Domain**: scientific
- **Entity Types**: Task, Method, Metric, Material, Generic
- **Year**: 2018
- **Format**: JSONL
- **License**: Apache-2.0 (SPDX)
- **Citation**: Luan et al. (2018)
- **Paper**: <https://aclanthology.org/D18-1360/>
- **Notes**: Scientific IE; paper abstracts with nested entities
- **URL**: <https://github.com/allenai/sciie>

### WebNLG

**Rust ID**: `DatasetId::WEBNLG`

Web NLG Challenge dataset. RDF-to-text generation with entity-relation triples.

- **Language**: en
- **Domain**: wikipedia
- **Entity Types**: Entity
- **Year**: 2017
- **Format**: XML
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Gardent et al. (2017)
- **Paper**: <https://aclanthology.org/W17-3518/>
- **Notes**: RDF triples to natural language; 15 DBpedia categories
- **URL**: <https://gitlab.com/webnlg/challenge-2017>

### Akkadian UD

**Rust ID**: `DatasetId::AkkadianUD`

Universal Dependencies for Akkadian. Cuneiform texts from ancient Mesopotamia.

- **Language**: akk
- **Domain**: historical
- **Entity Types**: PER, LOC, GPE
- **Year**: 2020
- **Format**: CoNLLU
- **License**: CC-BY-SA-4.0 (SPDX)
- **Citation**: UD Akkadian Team
- **Notes**: Cuneiform script; extinct Semitic language
- **URL**: <https://universaldependencies.org/treebanks/akk_pisandub/index.html>

### Ancient Hebrew UD

**Rust ID**: `DatasetId::AncientHebrewUD`

Universal Dependencies for Biblical Hebrew. Hebrew Bible text.

- **Language**: hbo
- **Domain**: religious
- **Entity Types**: PER, LOC, GPE
- **Year**: 2019
- **Format**: CoNLLU
- **License**: CC-BY-SA-4.0 (SPDX)
- **Citation**: UD Hebrew Team
- **Notes**: Biblical Hebrew; Torah and Prophets
- **URL**: <https://universaldependencies.org/treebanks/hbo_ptnk/index.html>

### Classical Chinese UD

**Rust ID**: `DatasetId::ClassicalChineseUD`

Universal Dependencies for Classical/Literary Chinese. Pre-modern texts.

- **Language**: lzh
- **Domain**: historical
- **Entity Types**: PER, LOC, GPE
- **Year**: 2018
- **Format**: CoNLLU
- **License**: CC-BY-SA-4.0 (SPDX)
- **Citation**: UD Classical Chinese Team
- **Notes**: Literary Chinese; classical texts and commentaries
- **URL**: <https://universaldependencies.org/treebanks/lzh_kyoto/index.html>

### Coptic UD

**Rust ID**: `DatasetId::CopticUD`

Universal Dependencies for Coptic. Late Egyptian language.

- **Language**: cop
- **Domain**: religious
- **Entity Types**: PER, LOC, GPE
- **Year**: 2016
- **Format**: CoNLLU
- **License**: CC-BY-SA-4.0 (SPDX)
- **Citation**: Zeldes & Schroeder (2016)
- **Notes**: Coptic; Gnostic and Biblical texts
- **URL**: <https://universaldependencies.org/treebanks/cop_scriptorium/index.html>

### Gothic UD

**Rust ID**: `DatasetId::GothicUD`

Universal Dependencies for Gothic. Wulfila's Bible translation.

- **Language**: got
- **Domain**: religious
- **Entity Types**: PER, LOC, GPE
- **Year**: 2014
- **Format**: CoNLLU
- **License**: CC-BY-NC-SA-4.0 (SPDX)
- **Citation**: PROIEL Team
- **Notes**: Gothic; oldest substantial Germanic text
- **URL**: <https://universaldependencies.org/treebanks/got_proiel/index.html>

### Hittite UD

**Rust ID**: `DatasetId::HittiteUD`

Universal Dependencies for Hittite. Ancient Anatolian language.

- **Language**: hit
- **Domain**: historical
- **Entity Types**: PER, LOC, GPE
- **Year**: 2021
- **Format**: CoNLLU
- **License**: CC-BY-SA-4.0 (SPDX)
- **Citation**: UD Hittite Team
- **Notes**: Cuneiform Hittite; Bronze Age Anatolia
- **URL**: <https://universaldependencies.org/treebanks/hit_hittb/index.html>

### Old Church Slavonic UD

**Rust ID**: `DatasetId::OldChurchSlavonicUD`

Universal Dependencies for OCS. Medieval Slavic liturgical language.

- **Language**: cu
- **Domain**: religious
- **Entity Types**: PER, LOC, GPE
- **Year**: 2014
- **Format**: CoNLLU
- **License**: CC-BY-NC-SA-4.0 (SPDX)
- **Citation**: PROIEL Team
- **Notes**: Oldest Slavic literary language; Cyrillic/Glagolitic
- **URL**: <https://universaldependencies.org/treebanks/cu_proiel/index.html>

### Latin ITTB

**Rust ID**: `DatasetId::LatinITTB`

Index Thomisticus Treebank. Medieval Latin theological texts.

- **Language**: la
- **Domain**: religious
- **Entity Types**: PER, LOC, ORG
- **Year**: 2009
- **Format**: CoNLLU
- **License**: CC-BY-NC-SA-3.0 (SPDX)
- **Citation**: McGillivray et al. (2009)
- **Notes**: Aquinas texts; medieval scholastic Latin
- **URL**: <https://universaldependencies.org/treebanks/la_ittb/index.html>

### Latin PROIEL

**Rust ID**: `DatasetId::LatinPROIEL`

Pragmatic Resources in Old Indo-European Languages. Classical Latin.

- **Language**: la
- **Domain**: historical
- **Entity Types**: PER, LOC, GPE
- **Year**: 2014
- **Format**: CoNLLU
- **License**: CC-BY-NC-SA-4.0 (SPDX)
- **Citation**: PROIEL Team
- **Notes**: Vulgate, Caesar, Cicero; classical and late Latin
- **URL**: <https://universaldependencies.org/treebanks/la_proiel/index.html>

### Esperanto UD

**Rust ID**: `DatasetId::EsperantoUD`

Universal Dependencies for Esperanto. Planned international language.

- **Language**: eo
- **Domain**: general
- **Entity Types**: PER, LOC, ORG
- **Year**: 2017
- **Format**: CoNLLU
- **License**: CC-BY-SA-4.0 (SPDX)
- **Citation**: UD Esperanto Team
- **Notes**: Constructed language; regular agglutinative morphology
- **URL**: <https://universaldependencies.org/treebanks/eo_pud/index.html>

### Dothraki

**Rust ID**: `DatasetId::Dothraki`

Dothraki language corpus. Game of Thrones constructed language.

- **Language**: dlk
- **Domain**: fiction
- **Entity Types**: PER, LOC
- **Year**: 2011
- **Format**: Custom
- **License**: CC-BY-SA-4.0 (SPDX)
- **Citation**: Peterson (2011)
- **Notes**: Conlang by David Peterson; SVO word order
- **URL**: <https://wiki.dothraki.org/>

### High Valyrian

**Rust ID**: `DatasetId::HighValyrian`

High Valyrian corpus. Game of Thrones constructed language.

- **Language**: hvy
- **Domain**: fiction
- **Entity Types**: PER, LOC
- **Year**: 2013
- **Format**: Custom
- **License**: CC-BY-SA-4.0 (SPDX)
- **Citation**: Peterson (2013)
- **Notes**: Highly inflected conlang; 4 genders, 8 cases
- **URL**: <https://wiki.dothraki.org/High_Valyrian>

### Klingon

**Rust ID**: `DatasetId::Klingon`

Klingon language corpus. Star Trek constructed language.

- **Language**: tlh
- **Domain**: fiction
- **Entity Types**: PER, LOC, ORG
- **Year**: 1985
- **Format**: Custom
- **License**: Research
- **Citation**: Okrand (1985)
- **Notes**: OVS word order; unique phonology; active community
- **URL**: <https://github.com/klingonlanguage/klingon-data>

### Quenya

**Rust ID**: `DatasetId::Quenya`

Quenya language corpus. Tolkien's Elvish language.

- **Language**: qya
- **Domain**: fiction
- **Entity Types**: PER, LOC
- **Year**: 1954
- **Format**: Custom
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Tolkien (1954)
- **Notes**: Finnish-inspired phonology; Tengwar script
- **URL**: <https://eldamo.org/>

### Na'vi

**Rust ID**: `DatasetId::Navi`

Na'vi language corpus. Avatar constructed language.

- **Language**: nav
- **Domain**: fiction
- **Entity Types**: PER, LOC
- **Year**: 2009
- **Format**: Custom
- **License**: Research
- **Citation**: Frommer (2009)
- **Notes**: Free word order; ejectives; infixes
- **URL**: <https://learnnavi.org/>

### Interslavic

**Rust ID**: `DatasetId::Interslavic`

Interslavic zonal auxiliary language. Constructed for Slavic intelligibility.

- **Language**: isv
- **Domain**: general
- **Entity Types**: PER, LOC, ORG
- **Year**: 2006
- **Format**: Custom
- **License**: CC-BY-SA-4.0 (SPDX)
- **Citation**: Interslavic Team (2006)
- **Notes**: Maximizes mutual intelligibility across Slavic languages
- **URL**: <https://interslavic.fun/>

### Lojban

**Rust ID**: `DatasetId::Lojban`

Lojban logical language corpus. Constructed for unambiguous communication.

- **Language**: jbo
- **Domain**: general
- **Year**: 1997
- **Format**: Custom
- **License**: Public Domain
- **Citation**: Cowan (1997)
- **Notes**: Predicate logic-based; completely unambiguous grammar
- **URL**: <https://mw.lojban.org/>

### Toki Pona

**Rust ID**: `DatasetId::TokiPona`

Toki Pona minimalist language corpus. 120-word philosophical language.

- **Language**: tok
- **Domain**: general
- **Year**: 2001
- **Format**: Custom
- **License**: CC-BY-SA-4.0 (SPDX)
- **Citation**: Lang (2001)
- **Notes**: Minimalist; tests compositional semantics
- **URL**: <https://github.com/kilipan/toki-pona-corpus>

### i2b2-2010

**Rust ID**: `DatasetId::I2B22010`

i2b2/VA 2010 NLP Challenge. Clinical concept extraction and relations.

- **Language**: en
- **Domain**: clinical
- **Entity Types**: Problem, Treatment, Test
- **Year**: 2010
- **Format**: Custom
- **License**: DUA Required
- **Citation**: Uzuner et al. (2011)
- **Paper**: <https://academic.oup.com/jamia/article/18/5/552/830538>
- **Notes**: Clinical notes; concept and relation extraction
- **URL**: <https://www.i2b2.org/NLP/DataSets/>

### i2b2 De-identification

**Rust ID**: `DatasetId::I2b2Deidentification`

i2b2 2014 De-identification Challenge. PHI recognition and removal.

- **Language**: en
- **Domain**: clinical
- **Entity Types**: Name, Date, Address, Phone, SSN, MRN
- **Year**: 2014
- **Format**: Custom
- **License**: DUA Required
- **Citation**: Stubbs et al. (2015)
- **Notes**: PHI de-identification; HIPAA compliance
- **URL**: <https://www.i2b2.org/NLP/DataSets/>

### French Clinical NER

**Rust ID**: `DatasetId::FrenchClinicalNER`

French clinical NER from hospital records. APHP collaboration.

- **Language**: fr
- **Domain**: clinical
- **Entity Types**: Drug, Disease, Procedure, Date
- **Year**: 2022
- **Format**: Standoff
- **License**: DUA Required
- **Citation**: APHP Team (2022)
- **Notes**: French clinical text; covers multiple entity types
- **URL**: <https://github.com/EDS-NLP/eds-nlp>

### ShARe/CLEF 2013

**Rust ID**: `DatasetId::ShARe13`

ShARe/CLEF eHealth 2013. Disorder mention recognition.

- **Language**: en
- **Domain**: clinical
- **Entity Types**: Disorder
- **Year**: 2013
- **Format**: Standoff
- **License**: PhysioNet
- **Citation**: Suominen et al. (2013)
- **Notes**: Clinical disorder identification; SNOMED CT normalization
- **URL**: <https://physionet.org/content/shareclefehealth2013/>

### ShARe/CLEF 2014

**Rust ID**: `DatasetId::ShARe14`

ShARe/CLEF eHealth 2014. Improved disorder normalization.

- **Language**: en
- **Domain**: clinical
- **Entity Types**: Disorder
- **Year**: 2014
- **Format**: Standoff
- **License**: PhysioNet
- **Citation**: Mowery et al. (2014)
- **Notes**: Extended from 2013; template filling and normalization
- **URL**: <https://physionet.org/content/shareclefehealth2014/>

### CALCS

**Rust ID**: `DatasetId::CALCS`

Computational Approaches to Linguistic Code-Switching. Multiple language pairs.

- **Language**: multi
- **Domain**: social_media
- **Entity Types**: PER, LOC, ORG
- **Year**: 2018
- **Format**: CoNLL
- **License**: Research
- **Citation**: CALCS Workshop
- **Notes**: Code-switching NER; Spanish-English, Hindi-English
- **URL**: <https://code-switching.github.io/>

### LinCE

**Rust ID**: `DatasetId::LinCE`

Linguistic Code-switching Evaluation. Multiple code-switching benchmarks.

- **Language**: multi
- **Domain**: social_media
- **Entity Types**: PER, LOC, ORG
- **Year**: 2020
- **Format**: CoNLL
- **License**: Research
- **Citation**: Aguilar et al. (2020)
- **Paper**: <https://aclanthology.org/2020.lrec-1.223/>
- **Notes**: Spanish-English, Hindi-English; includes NER task
- **URL**: <https://ritual.uh.edu/lince/>

### GLUECoS

**Rust ID**: `DatasetId::GLUECoS`

Code-Switching GLUE benchmark. NLU for code-switched text.

- **Language**: multi
- **Domain**: social_media
- **Entity Types**: PER, LOC, ORG
- **Year**: 2020
- **Format**: JSONL
- **License**: MIT (SPDX)
- **Citation**: Khanuja et al. (2020)
- **Paper**: <https://aclanthology.org/2020.emnlp-main.574/>
- **Notes**: Hindi-English and Spanish-English; NLU tasks
- **URL**: <https://github.com/microsoft/GLUECoS>

### ChemDataExtractor

**Rust ID**: `DatasetId::ChemDataExtractor`

Chemical data extraction toolkit benchmark. Chemical NER and properties.

- **Language**: en
- **Domain**: biomedical
- **Entity Types**: Chemical, Property, Value, Unit
- **Year**: 2016
- **Format**: Custom
- **License**: MIT (SPDX)
- **Citation**: Swain & Cole (2016)
- **Notes**: Chemical property extraction; materials science
- **URL**: <https://chemdataextractor.org/>

### HUPD

**Rust ID**: `DatasetId::HUPD`

Harvard USPTO Patent Dataset. Patent application NER.

- **Language**: en
- **Domain**: legal
- **Entity Types**: Inventor, Assignee, Reference, Claim
- **Year**: 2022
- **Format**: JSONL
- **License**: Public Domain
- **Citation**: Suzgun et al. (2022)
- **Notes**: Patent applications; technical language
- **URL**: <https://github.com/suzgunmirac/hupd>

### FinTech Patent NER

**Rust ID**: `DatasetId::FinTechPatent`

FinTech patent entity extraction. Financial technology domain.

- **Language**: en
- **Domain**: financial
- **Entity Types**: Technology, Company, Product, Method
- **Year**: 2021
- **Format**: CoNLL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: FinTech NER Team (2021)
- **Notes**: FinTech patents; specialized terminology
- **URL**: <https://github.com/fintech-patent-ner>

### WaterAgriNER

**Rust ID**: `DatasetId::WaterAgriNER`

Water and agriculture domain NER. Environmental science entities.

- **Language**: en
- **Domain**: scientific
- **Entity Types**: Crop, Chemical, Equipment, Location
- **Year**: 2022
- **Format**: CoNLL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: WaterAgriNER Team (2022)
- **Notes**: Agricultural and water management domains
- **URL**: <https://github.com/wateragriner>

### WIESP Astrophysics

**Rust ID**: `DatasetId::WIESPAstro`

WIESP 2022 Astrophysics NER. NASA ADS literature.

- **Language**: en
- **Domain**: astrophysics
- **Entity Types**: Mission, Instrument, CelestialObject, Phenomenon
- **Year**: 2022
- **Format**: JSONL
- **License**: Research
- **Citation**: WIESP Team (2022)
- **Notes**: Astrophysics entities; 31 fine-grained types
- **URL**: <https://ui.adsabs.harvard.edu/>

### NER Social Food

**Rust ID**: `DatasetId::NERsocialFood`

Food-related NER from social media. Recipes and food mentions.

- **Language**: en
- **Domain**: food
- **Entity Types**: Food, Ingredient, Brand, Restaurant
- **Year**: 2021
- **Format**: CoNLL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Food NER Team (2021)
- **Notes**: Social media food mentions; informal language
- **URL**: <https://github.com/food-ner/social>

### Russian Cultural NER

**Rust ID**: `DatasetId::RussianCulturalNER`

Russian cultural heritage NER. Museums, artworks, cultural entities.

- **Language**: ru
- **Domain**: encyclopedia
- **Entity Types**: Artwork, Artist, Museum, Period, Style
- **Year**: 2022
- **Format**: CoNLL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: RuCultural Team (2022)
- **Notes**: Russian cultural heritage; fine-grained art types
- **URL**: <https://github.com/russian-cultural-ner>

### 18th Century NER

**Rust ID**: `DatasetId::EighteenthCenturyNER`

Named entities in 18th century English text. Historical OCR challenges.

- **Language**: en
- **Domain**: historical
- **Entity Types**: PER, LOC, ORG, DATE
- **Year**: 2020
- **Format**: CoNLL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Living with Machines (2020)
- **Notes**: OCR noise; historical spelling variation
- **URL**: <https://github.com/Living-with-machines/>

### Spanish Medieval TEI

**Rust ID**: `DatasetId::SpanishMedievalTEI`

Medieval Spanish manuscript NER. TEI-encoded historical texts.

- **Language**: es
- **Domain**: historical
- **Entity Types**: PER, LOC, ORG, DATE
- **Year**: 2021
- **Format**: XML
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Spanish Medieval NLP (2021)
- **Notes**: Medieval Castilian; paleographic challenges
- **URL**: <https://github.com/spanish-medieval-nlp>

### Medieval Czech Charters

**Rust ID**: `DatasetId::MedievalCzechCharters`

Czech medieval charter NER. Historical legal documents.

- **Language**: cs
- **Domain**: historical
- **Entity Types**: PER, LOC, ORG, DATE
- **Year**: 2020
- **Format**: XML
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Czech Charter Team (2020)
- **Notes**: Medieval Czech and Latin; charter formulae
- **URL**: <https://github.com/czech-medieval-charters>

### Dutch Archaeology NER (v2)

**Rust ID**: `DatasetId::DutchArchaeologyNER`

Dutch archaeological excavation reports. DANS archive annotations.

- **Language**: nl
- **Domain**: archaeology
- **Entity Types**: Site, Artifact, Period, Material
- **Year**: 2021
- **Format**: Standoff
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: DANS (2021)
- **Notes**: Archaeological domain; ~31k annotations
- **URL**: <https://easy.dans.knaw.nl/>

### Guaraní NER

**Rust ID**: `DatasetId::GuaraniNER`

Guaraní language NER. South American indigenous language.

- **Language**: gn
- **Domain**: general
- **Entity Types**: PER, LOC, ORG
- **Year**: 2021
- **Format**: CoNLL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Guaraní NLP Team (2021)
- **Notes**: Low-resource indigenous language; Paraguay official language
- **URL**: <https://github.com/guarani-nlp>

### Shipibo-Konibo NER

**Rust ID**: `DatasetId::ShipiboKoniboNER`

Shipibo-Konibo language NER. Peruvian Amazonian language.

- **Language**: shp
- **Domain**: general
- **Entity Types**: PER, LOC, ORG
- **Year**: 2018
- **Format**: CoNLL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Mager et al. (2018)
- **Notes**: Endangered language; ~3k speakers
- **URL**: <https://github.com/ixa-ehu/shipibo-konibo>

### Navajo Morphology

**Rust ID**: `DatasetId::NavajoMorph`

Navajo morphological annotation. North American indigenous language.

- **Language**: nv
- **Domain**: general
- **Entity Types**: PER, LOC
- **Year**: 2020
- **Format**: CoNLLU
- **License**: Research
- **Citation**: Navajo NLP Team (2020)
- **Notes**: Complex verb morphology; tonal language
- **URL**: <https://github.com/navajo-nlp>

### KoCoNovel

**Rust ID**: `DatasetId::KoCoNovel`

Korean character coreference in 50 modern/contemporary novels. First Korean literary coreference dataset. Four versions: Reader/Omniscient perspective × Separate/Overlapped entity treatment. 178K tokens, 19K mentions, ~1.4K entities.

- **Language**: ko
- **Domain**: fiction
- **Entity Types**: PER
- **Year**: 2024
- **Format**: CoNLL
- **Annotation Scheme**: CoNLLCoref
- **Size**: 178K tokens, 50 novels
- **License**: CC-BY-SA-4.0 (SPDX)
- **Citation**: Kim, Lee & Lee (2024)
- **Paper**: <https://arxiv.org/abs/2404.01140>
- **Notes**: 24% of mentions are single common nouns (Korean address term culture favors kinship/title over names). Korean lacks determiners and proper noun markers. Four annotation versions available. Speaker annotations for direct quotations. IAA: MUC 94.53 F1.
- **URL**: <https://github.com/storidient/KoCoNovel>

### OpenBoek

**Rust ID**: `DatasetId::OpenBoek`

Dutch literary coreference. Open-source Dutch fiction annotation.

- **Language**: nl
- **Domain**: fiction
- **Entity Types**: PER, LOC, ORG
- **Year**: 2021
- **Format**: CoNLL
- **Annotation Scheme**: CoNLLCoref
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: OpenBoek Team (2021)
- **Notes**: Dutch novels; literary coreference patterns
- **URL**: <https://github.com/cltl/OpenBoek>

### SciCo

**Rust ID**: `DatasetId::SciCo`

Scientific coreference. Cross-document concept coreference in AI papers.

- **Language**: en
- **Domain**: scientific
- **Entity Types**: Method, Task, Dataset
- **Year**: 2021
- **Format**: JSONL
- **License**: Apache-2.0 (SPDX)
- **Citation**: Cattan et al. (2021)
- **Paper**: <https://aclanthology.org/2021.emnlp-main.518/>
- **Notes**: Scientific concepts; cross-document coreference
- **URL**: <https://github.com/allenai/scico>

### SemEval-2013 Task 9.1

**Rust ID**: `DatasetId::SemEval2013Task91`

Drug-drug interaction extraction. SemEval shared task.

- **Language**: en
- **Domain**: biomedical
- **Entity Types**: Drug, Drug_n, Group, Brand
- **Year**: 2013
- **Format**: XML
- **License**: Research
- **Citation**: Segura-Bedmar et al. (2013)
- **Paper**: <https://aclanthology.org/S13-2056/>
- **Notes**: Drug-drug interaction; MedLine and DrugBank
- **URL**: <https://www.cs.york.ac.uk/semeval-2013/task9/>

### PDTB 3.0 (v2)

**Rust ID**: `DatasetId::PDTB3`

Penn Discourse Treebank 3.0. Discourse relations and connectives.

- **Language**: en
- **Domain**: news
- **Year**: 2019
- **Format**: Custom
- **License**: LDC
- **Citation**: Prasad et al. (2019)
- **Notes**: Discourse relations; implicit and explicit connectives
- **URL**: <https://catalog.ldc.upenn.edu/LDC2019T05>

### WinoPron

**Rust ID**: `DatasetId::WinoPron`

Winograd pronoun resolution. Commonsense coreference benchmark.

- **Language**: en
- **Domain**: evaluation
- **Entity Types**: PER
- **Year**: 2021
- **Format**: Custom
- **License**: Research
- **Citation**: Davis & Marcus (2021)
- **Notes**: Extended Winograd schemas; commonsense reasoning
- **URL**: <https://cs.nyu.edu/~davise/papers/WinoPron/>

### QUOREF

**Rust ID**: `DatasetId::QUOREF`

Question answering requiring coreference. Reading comprehension.

- **Language**: en
- **Domain**: wikipedia
- **Entity Types**: PER, LOC, ORG
- **Year**: 2019
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Dasigi et al. (2019)
- **Paper**: <https://aclanthology.org/D19-1606/>
- **Notes**: QA requiring coreference resolution; Wikipedia paragraphs
- **URL**: <https://github.com/allenai/quoref>

### CoNLL-2002 Dutch

**Rust ID**: `DatasetId::CoNLL2002Dutch`

Dutch portion of CoNLL-2002 NER shared task. Newspaper text.

- **Language**: nl
- **Domain**: news
- **Entity Types**: PER, LOC, ORG, MISC
- **Year**: 2002
- **Format**: CoNLL
- **Annotation Scheme**: BIO
- **License**: Research
- **Citation**: Tjong Kim Sang (2002)
- **Paper**: <https://aclanthology.org/W02-2024/>
- **Notes**: Dutch newspaper NER; includes gazetteers
- **URL**: <https://www.clips.uantwerpen.be/conll2002/ner/data/ned.testa>

### CoNLL-2002 Spanish

**Rust ID**: `DatasetId::CoNLL2002Spanish`

Spanish portion of CoNLL-2002 NER shared task. News articles.

- **Language**: es
- **Domain**: news
- **Entity Types**: PER, LOC, ORG, MISC
- **Year**: 2002
- **Format**: CoNLL
- **Annotation Scheme**: BIO
- **License**: Research
- **Citation**: Tjong Kim Sang (2002)
- **Paper**: <https://aclanthology.org/W02-2024/>
- **Notes**: Spanish EFE news agency articles
- **URL**: <https://www.clips.uantwerpen.be/conll2002/ner/data/esp.testa>

### BC2GM Full

**Rust ID**: `DatasetId::BC2GMFull`

Complete BioCreative II Gene Mention corpus. Extended from BC2GM.

- **Language**: en
- **Domain**: biomedical
- **Entity Types**: Gene, Protein
- **Year**: 2008
- **Format**: IOB2
- **License**: Research
- **Citation**: Smith et al. (2008)
- **Notes**: Full corpus including training data
- **URL**: <https://biocreative.bioinformatics.udel.edu/resources/biocreative-ii-corpus/>

### FinNER

**Rust ID**: `DatasetId::FinNER`

Finnish named entity recognition. News and Wikipedia text.

- **Language**: fi
- **Domain**: news
- **Entity Types**: PER, LOC, ORG, DATE, EVENT
- **Year**: 2020
- **Format**: CoNLL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Ruokolainen et al. (2020)
- **Notes**: Finnish morphologically rich language NER
- **URL**: <https://github.com/mpsilfern/finer>

### LegalNER

**Rust ID**: `DatasetId::LegalNER`

Legal Named Entity Recognition. Court cases and legislation.

- **Language**: en
- **Domain**: legal
- **Entity Types**: Court, Judge, Lawyer, Party, Statute, Case
- **Year**: 2021
- **Format**: CoNLL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: LegalNER Team (2021)
- **Notes**: Legal domain entities; US court documents
- **URL**: <https://github.com/legal-ner/legal-ner>

### CEREC

**Rust ID**: `DatasetId::CEREC`

Chinese entity and relation extraction corpus. Web text and news.

- **Language**: zh
- **Domain**: news
- **Entity Types**: PER, LOC, ORG
- **Year**: 2021
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Huang et al. (2021)
- **Notes**: Chinese NER and RE; includes nested entities
- **URL**: <https://github.com/Stardust-hyx/CEREC>

### DELICATE

**Rust ID**: `DatasetId::DELICATE`

Depression, emotion, and linguistic analysis corpus. Mental health text.

- **Language**: en
- **Domain**: clinical
- **Entity Types**: Symptom, Treatment, Emotion
- **Year**: 2022
- **Format**: JSONL
- **License**: Research
- **Citation**: DELICATE Team (2022)
- **Notes**: Mental health NER; sensitive domain
- **URL**: <https://github.com/delicate-nlp/delicate>

### SciERC NER

**Rust ID**: `DatasetId::SciERCNER`

Scientific Information Extraction NER. AI paper abstracts.

- **Language**: en
- **Domain**: scientific
- **Entity Types**: Task, Method, Metric, Material, OtherScientificTerm, Generic
- **Year**: 2018
- **Format**: JSONL
- **License**: Apache-2.0 (SPDX)
- **Citation**: Luan et al. (2018)
- **Paper**: <https://aclanthology.org/D18-1360/>
- **Notes**: 6 entity types; includes nested entities and coreference
- **URL**: <https://github.com/allenai/sciie/tree/main/data>

### ULNER

**Rust ID**: `DatasetId::ULNER`

Ultra-Large Scale NER. Massive silver-standard dataset.

- **Language**: en
- **Domain**: mixed
- **Entity Types**: PER, LOC, ORG, MISC
- **Year**: 2023
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: ULNER Team (2023)
- **Notes**: Large-scale distantly supervised NER
- **URL**: <https://huggingface.co/datasets/ULNER>

### UniversalNER

**Rust ID**: `DatasetId::UniversalNER`

Universal NER model benchmark. Multiple domains and languages.

- **Language**: multi
- **Domain**: mixed
- **Entity Types**: PER, LOC, ORG
- **Year**: 2023
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Zhou et al. (2023)
- **Paper**: <https://arxiv.org/abs/2308.03279>
- **Notes**: ChatGPT-distilled NER model benchmark
- **URL**: <https://universal-ner.github.io/>

### ARRAU GENIA

**Rust ID**: `DatasetId::ArrauGenia`

ARRAU corpus GENIA portion. Biomedical coreference.

- **Language**: en
- **Domain**: biomedical
- **Entity Types**: Gene, Protein, Cell
- **Year**: 2020
- **Format**: MMAX2
- **Annotation Scheme**: ARRAU
- **License**: Research
- **Citation**: Uryupina et al. (2020)
- **Notes**: Biomedical portion of ARRAU corpus
- **URL**: <https://aclanthology.org/2020.codi-1.1/>

### ARRAU Pear Stories

**Rust ID**: `DatasetId::ArrauPear`

ARRAU Pear Stories portion. Narrative coreference.

- **Language**: en
- **Domain**: narrative
- **Entity Types**: PER, LOC, ORG
- **Year**: 2020
- **Format**: MMAX2
- **Annotation Scheme**: ARRAU
- **License**: Research
- **Citation**: Uryupina et al. (2020)
- **Notes**: Film retelling narratives; discourse structure
- **URL**: <https://aclanthology.org/2020.codi-1.1/>

### ARRAU RST

**Rust ID**: `DatasetId::ArrauRst`

ARRAU RST-DT portion. Discourse-annotated Wall Street Journal.

- **Language**: en
- **Domain**: news
- **Entity Types**: PER, LOC, ORG
- **Year**: 2020
- **Format**: MMAX2
- **Annotation Scheme**: ARRAU
- **License**: Research
- **Citation**: Uryupina et al. (2020)
- **Notes**: WSJ with RST discourse structure
- **URL**: <https://aclanthology.org/2020.codi-1.1/>

### ARRAU Trains

**Rust ID**: `DatasetId::ArrauTrains`

ARRAU Trains portion. Task-oriented dialogue coreference.

- **Language**: en
- **Domain**: dialogue
- **Entity Types**: PER, LOC, TIME
- **Year**: 2020
- **Format**: MMAX2
- **Annotation Scheme**: ARRAU
- **License**: Research
- **Citation**: Uryupina et al. (2020)
- **Notes**: Task-oriented dialogue; train scheduling domain
- **URL**: <https://aclanthology.org/2020.codi-1.1/>

### NomBank Implicit

**Rust ID**: `DatasetId::NomBankImplicit`

Implicit arguments in NomBank. Nominal predicate-argument structures.

- **Language**: en
- **Domain**: news
- **Year**: 2012
- **Format**: Custom
- **License**: LDC
- **Citation**: Gerber & Chai (2012)
- **Notes**: Implicit argument recovery; extends NomBank
- **URL**: <https://catalog.ldc.upenn.edu/LDC2008T23>

### BASHI

**Rust ID**: `DatasetId::BASHI`

Bangla Shared Task on Information extraction. Bengali NER.

- **Language**: bn
- **Domain**: news
- **Entity Types**: PER, LOC, ORG
- **Year**: 2020
- **Format**: CoNLL
- **License**: Research
- **Citation**: BASHI Team (2020)
- **Notes**: Bengali (Bangla) NER; low-resource setting
- **URL**: <https://sites.google.com/view/ipm-bashi/>

### ERST

**Rust ID**: `DatasetId::ERST`

English RST Signalling Corpus. Discourse markers and signals.

- **Language**: en
- **Domain**: mixed
- **Year**: 2018
- **Format**: Custom
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Das & Taboada (2018)
- **Notes**: Discourse signals; extends RST-DT
- **URL**: <https://github.com/rsttools/signal>

### BiTimeBERT

**Rust ID**: `DatasetId::BiTimeBERT`

Bi-directional temporal relation dataset. Event ordering and duration.

- **Language**: en
- **Domain**: news
- **Entity Types**: Event, Time
- **Year**: 2022
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: BiTimeBERT Team (2022)
- **Notes**: Temporal reasoning; event-time relations
- **URL**: <https://github.com/btime-bert/bitimebert>

### TRIDIS

**Rust ID**: `DatasetId::TRIDIS`

Triple Discourse dataset. Entity and discourse relations.

- **Language**: en
- **Domain**: mixed
- **Entity Types**: PER, LOC, ORG
- **Year**: 2021
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: TRIDIS Team (2021)
- **Notes**: Combined entity and discourse annotation
- **URL**: <https://github.com/tridis/tridis>

### QueerBench

**Rust ID**: `DatasetId::QueerBench`

Queer identity coreference benchmark. LGBTQ+ representation in NLP.

- **Language**: en
- **Domain**: evaluation
- **Entity Types**: PER
- **Year**: 2022
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: QueerBench Team (2022)
- **Notes**: Tests coreference for non-binary pronouns; bias evaluation
- **URL**: <https://github.com/queerbench/queerbench>

### QUEEREOTYPES

**Rust ID**: `DatasetId::QUEEREOTYPES`

LGBTQ+ stereotype detection in text. Bias in language models.

- **Language**: en
- **Domain**: evaluation
- **Year**: 2023
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Felkner et al. (2023)
- **Notes**: Stereotype detection; tests model biases
- **URL**: <https://github.com/queereotypes/queereotypes>

### MAP

**Rust ID**: `DatasetId::MAP`

Medical Annotation Pipeline dataset. Clinical concept normalization.

- **Language**: en
- **Domain**: clinical
- **Entity Types**: Drug, Disease, Procedure
- **Year**: 2021
- **Format**: Standoff
- **License**: DUA Required
- **Citation**: MAP Team (2021)
- **Notes**: Clinical concept extraction and normalization
- **URL**: <https://github.com/medical-annotation-pipeline/map>

### ASN

**Rust ID**: `DatasetId::ASN`

Atomic Slot Number dataset. Slot filling benchmark.

- **Language**: en
- **Domain**: news
- **Entity Types**: Organization, Person, Date
- **Year**: 2013
- **Format**: Custom
- **License**: Research
- **Citation**: Law et al. (2013)
- **Notes**: Atomic slot filling; relation extraction
- **URL**: <http://www.cs.toronto.edu/~varada/ASN/>

### CSN

**Rust ID**: `DatasetId::CSN`

Code Search Net. Programming language dataset for code understanding.

- **Language**: multi
- **Domain**: code
- **Entity Types**: Function, Class, Variable
- **Year**: 2019
- **Format**: JSONL
- **License**: MIT (SPDX)
- **Citation**: Husain et al. (2019)
- **Paper**: <https://arxiv.org/abs/1909.09436>
- **Notes**: Code entity and function extraction; 6 languages
- **URL**: <https://github.com/github/CodeSearchNet>

### HOMOMEX

**Rust ID**: `DatasetId::HOMOMEX`

Homonym resolution in Mexican Spanish. Word sense disambiguation.

- **Language**: es
- **Domain**: general
- **Year**: 2021
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: HOMOMEX Team (2021)
- **Notes**: Mexican Spanish; tests regional variation
- **URL**: <https://github.com/homomex/homomex>

### ENER

**Rust ID**: `DatasetId::ENER`

E-commerce NER. Product entities in e-commerce text.

- **Language**: en
- **Domain**: general
- **Entity Types**: Product, Brand, Attribute, Price
- **Year**: 2022
- **Format**: CoNLL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: ENER Team (2022)
- **Notes**: E-commerce domain; product catalogs
- **URL**: <https://github.com/ener-dataset/ener>

### FIREBALL

**Rust ID**: `DatasetId::FIREBALL`

D&D gameplay NLG with true game state. ~25k sessions, 153k turns with structured game state.

- **Language**: en
- **Domain**: gaming
- **Entity Types**: Character, Item, Location, Creature, Spell, Action
- **Year**: 2020
- **Format**: JSONL
- **Size**: ~25k sessions, 153k turns
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Rameshkumar & Bailey (2020)
- **Paper**: <https://par.nsf.gov/biblio/10463286>
- **Notes**: D&D actual play with structured game state; tests NLG in narrative gaming
- **URL**: <https://huggingface.co/datasets/lara-martin/FIREBALL>

### D&D NER Benchmark

**Rust ID**: `DatasetId::DnDNERBenchmark`

Fantasy NER from 7 D&D adventure books. LLM-annotated fantasy entities.

- **Language**: en
- **Domain**: gaming
- **Entity Types**: Character, Location, Item, Creature, Spell, Organization
- **Year**: 2023
- **Format**: CoNLL
- **License**: Research
- **Citation**: Veselovsky et al. (2023)
- **Paper**: <https://aclanthology.org/2023.ranlp-1.130/>
- **Notes**: Fantasy domain; Flair/Trankit/SpaCy benchmarks; tests fictional entity recognition
- **URL**: <https://aclanthology.org/2023.ranlp-1.130.pdf>

### Critical Role Dataset

**Rust ID**: `DatasetId::CriticalRoleDataset`

Unscripted live D&D transcripts. Storytelling and dialogue analysis.

- **Language**: en
- **Domain**: gaming
- **Entity Types**: Character, Location, Item
- **Year**: 2020
- **Format**: Custom
- **License**: Research
- **Citation**: Rameshkumar & Bailey (2020)
- **Paper**: <https://aclanthology.org/2020.acl-main.459/>
- **Notes**: Live improvised gameplay transcripts; narrative coherence and character tracking
- **URL**: <https://www.microsoft.com/en-us/research/wp-content/uploads/2020/06/R.Rameshkumar-and-P.Bailey-Storytelling-with-Dialogue-ACL2020.pdf>

### CUAD

**Rust ID**: `DatasetId::CUAD`

Contract Understanding Atticus Dataset. 13k+ labels across 510 commercial contracts.

- **Language**: en
- **Domain**: legal
- **Entity Types**: Party, Date, Amount, Clause, Jurisdiction
- **Year**: 2021
- **Format**: JSONL
- **Size**: 510 contracts, 13k+ annotations, 41 clause types
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Hendrycks et al. (2021)
- **Paper**: <https://arxiv.org/abs/2103.06268>
- **Notes**: Contract clause extraction; covers indemnification, IP, termination clauses
- **URL**: <https://www.atticusprojectai.org/cuad>

### ACORD

**Rust ID**: `DatasetId::ACORD`

Expert-annotated clause retrieval for contract drafting. 114 queries, 126k+ pairs.

- **Language**: en
- **Domain**: legal
- **Entity Types**: Clause, Party, Obligation, Condition
- **Year**: 2025
- **Format**: JSONL
- **Size**: 114 queries, 126k+ query-clause pairs with 1-5 star rankings
- **License**: Research
- **Citation**: ACORD Team (2025)
- **Paper**: <https://arxiv.org/abs/2501.06582>
- **Notes**: Clause retrieval; Limitation of Liability, Indemnification, MFN clauses
- **URL**: <https://arxiv.org/html/2501.06582v1>

### Party Extraction Dataset

**Rust ID**: `DatasetId::PartyExtractionDataset`

Legal party identification from contracts. Contextual span representations.

- **Language**: en
- **Domain**: legal
- **Entity Types**: Party, Role, Organization
- **Year**: 2023
- **Format**: Standoff
- **License**: Research
- **Citation**: Tuggener et al. (2023)
- **Paper**: <https://aclanthology.org/2023.ranlp-1.116/>
- **Notes**: Legal party NER; disambiguates parties in complex contract structures
- **URL**: <https://aclanthology.org/2023.ranlp-1.116.pdf>

### FINER (Food)

**Rust ID**: `DatasetId::FINERFood`

Food ingredient NER. 181k ingredient phrases in IOB2 format.

- **Language**: en
- **Domain**: food
- **Entity Types**: Ingredient, Product, Quantity, Unit, State
- **Year**: 2022
- **Format**: BIO
- **Size**: 181,970 ingredient phrases
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Popovski et al. (2022)
- **Notes**: Semi-supervised multi-model prediction for ingredient parsing
- **URL**: <https://figshare.com/articles/dataset/Food_Ingredient_Named-Entity_Data/20222361>

### NHK Recipe Dataset

**Rust ID**: `DatasetId::NHKRecipeDataset`

Japanese recipes with ingredient state tracking across cooking steps.

- **Language**: ja
- **Domain**: food
- **Entity Types**: Ingredient, Action, State, Tool
- **Year**: 2025
- **Format**: JSONL
- **License**: Research
- **Citation**: NHK Team (2025)
- **Paper**: <https://arxiv.org/abs/2507.17232>
- **Notes**: State transitions per ingredient; procedural understanding in Japanese
- **URL**: <https://arxiv.org/html/2507.17232v1>

### Sanskrit NER (Bhagavad Gita)

**Rust ID**: `DatasetId::SanskritNERBhagavadGita`

Sanskrit NER from Bhagavad Gita and Patanjali Yoga Sutras.

- **Language**: sa
- **Domain**: religious
- **Entity Types**: PER, LOC, ORG, CONCEPT
- **Year**: 2025
- **Format**: CoNLL
- **License**: Research
- **Citation**: Suklabaidya (2025)
- **Notes**: Classical Sanskrit texts; tests Indic script and religious terminology
- **URL**: <https://www.kaggle.com/datasets/akashsuklabaidya/ner-dataset-fyp-25>

### Akkadian Cuneiform Dataset

**Rust ID**: `DatasetId::AkkadianCuneiformDataset`

Unicode cuneiform with transliteration. Old/Middle Babylonian, Neo-Assyrian.

- **Language**: akk
- **Domain**: historical
- **Entity Types**: Person, Place, God, Object
- **Year**: 2020
- **Format**: Custom
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Gordin et al. (2020)
- **Paper**: <https://pmc.ncbi.nlm.nih.gov/articles/PMC7592802/>
- **Notes**: Cuneiform glyphs with segmentation; covers ~2000 years of Mesopotamian text
- **URL**: <https://pmc.ncbi.nlm.nih.gov/articles/PMC7592802/>

### Heidelberg Cuneiform Benchmark

**Rust ID**: `DatasetId::HeidelbergCuneiformBenchmark`

Cuneiform sign classification across historical periods.

- **Language**: akk
- **Domain**: historical
- **Entity Types**: Sign, Determinative, Logogram
- **Year**: 2023
- **Format**: Custom
- **License**: Research
- **Citation**: Heidelberg Team (2023)
- **Paper**: <https://direct.mit.edu/coli/article/49/3/703/116160>
- **Notes**: Sign-level classification; tests paleographic variation across periods
- **URL**: <https://direct.mit.edu/coli/article/49/3/703/116160>

### Greek Mythology Knowledge Graph

**Rust ID**: `DatasetId::GreekMythologyKG`

Coref + RE pipeline for mythological texts. 15k+ entities from Roscher's Lexikon.

- **Language**: en
- **Domain**: mythology
- **Entity Types**: Deity, Hero, Place, Creature, Object, Event
- **Year**: 2019
- **Format**: Custom
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Myth KG Team (2019)
- **Paper**: <https://www.semantic-web-journal.net/content/greek-mythology-knowledge-graph>
- **Notes**: RDF conversion of mythological texts; handles divine genealogies and epithets
- **URL**: <https://www.semantic-web-journal.net/system/files/swj2754.pdf>

### Folklore Motif Distribution

**Rust ID**: `DatasetId::FolkloreMotifDistribution`

548 folklore motifs across 309 ethnic traditions in the Old World.

- **Language**: multi
- **Domain**: mythology
- **Entity Types**: Motif, Tradition, Region, Character
- **Year**: 2015
- **Format**: Custom
- **License**: Research
- **Citation**: Berezkin et al. (2015)
- **Notes**: Cross-cultural motif tracking; tests cultural entity alignment
- **URL**: <https://www.academia.edu/14481230/>

### ND-NER

**Rust ID**: `DatasetId::NDNER`

National defense OSINT NER. 17+ entity types for military equipment.

- **Language**: en
- **Domain**: defense
- **Entity Types**: AIRCRAFT, SHIP, MISSILE, TANK, FIREARM, ELECTRONIC, MASS_DESTR, SPACE, NEW
- **Year**: 2022
- **Format**: CoNLL
- **License**: CC-BY-SA-4.0 (SPDX)
- **Citation**: Li et al. (2022)
- **Notes**: Nested and flat versions; covers WMDs, directed energy, kinetic weapons
- **URL**: <https://github.com/XinyanLi2016/ND-NER>

### re3d (Defense)

**Rust ID**: `DatasetId::Re3dDefense`

Relationship and Entity Extraction Evaluation Dataset for defense domain.

- **Language**: en
- **Domain**: defense
- **Entity Types**: Person, Organization, Location, Equipment, Event
- **Year**: 2016
- **Format**: BRAT
- **License**: OGL
- **Citation**: DSTL (2016)
- **Notes**: UK Defence Science; relationship extraction for intelligence analysis
- **URL**: <https://github.com/dstl/re3d>

### CyNER-APTNER

**Rust ID**: `DatasetId::CyNERAptner`

Unified cyber threat intelligence NER. Malware, threat actors, IOCs.

- **Language**: en
- **Domain**: cybersecurity
- **Entity Types**: Malware, ThreatActor, Vulnerability, Indicator, Tool
- **Year**: 2024
- **Format**: CoNLL
- **License**: Research
- **Citation**: CyNER Team (2024)
- **Paper**: <https://ceur-ws.org/Vol-3928/paper_170.pdf>
- **Notes**: Merged cyber threat datasets; security bulletin extraction
- **URL**: <https://ceur-ws.org/Vol-3928/paper_170.pdf>

### Chinese Engineering Geology NER

**Rust ID**: `DatasetId::ChineseEngineeringGeologyNER`

Geological disasters NER with EDA-based augmentation for small samples.

- **Language**: zh
- **Domain**: geology
- **Entity Types**: Disaster, Location, Cause, Measure, Material
- **Year**: 2023
- **Format**: BIO
- **License**: Research
- **Citation**: Geology NER Team (2023)
- **Paper**: <https://doi.org/10.1016/j.eswa.2023.122427>
- **Notes**: Engineering geology reports; data augmentation for low-resource domain
- **URL**: <https://www.sciencedirect.com/science/article/abs/pii/S0957417423024272>

### LLM-RocMin-NER

**Rust ID**: `DatasetId::LLMRocMinNER`

Rocks and minerals NER. 2-shot prompt-based extraction with nested handling.

- **Language**: en
- **Domain**: geology
- **Entity Types**: Rock, Mineral, Element, Property, Location
- **Year**: 2025
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: RocMin Team (2025)
- **Paper**: <https://doi.org/10.1016/j.cageo.2025.105949>
- **Notes**: Few-shot geoscience NER; handles nested mineral compositions
- **URL**: <https://www.sciencedirect.com/science/article/abs/pii/S0098300425000949>

### PolyIE

**Rust ID**: `DatasetId::PolyIE`

Polymer materials NER + relation extraction from literature.

- **Language**: en
- **Domain**: materials
- **Entity Types**: Polymer, Property, Value, Condition, Method
- **Year**: 2024
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Shetty et al. (2024)
- **Paper**: <https://aclanthology.org/2024.naacl-long.131/>
- **Notes**: Polymer science literature; property-structure relationships
- **URL**: <https://ramprasad.mse.gatech.edu/PolyIE/>

### MathDial

**Rust ID**: `DatasetId::MathDial`

Teacher-student tutoring dialogues on multi-step math problems.

- **Language**: en
- **Domain**: education
- **Entity Types**: Student, Teacher, Problem, Step, Hint
- **Year**: 2023
- **Format**: JSONL
- **Size**: 3,000 tutoring dialogues
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Macina et al. (2023)
- **Paper**: <https://arxiv.org/abs/2305.14536>
- **Notes**: Scaffolding questions taxonomy; tests pedagogical dialogue understanding
- **URL**: <https://arxiv.org/abs/2305.14536>

### CoMTA

**Rust ID**: `DatasetId::CoMTA`

Student-GPT4 Khanmigo tutor dialogues for knowledge tracing.

- **Language**: en
- **Domain**: education
- **Entity Types**: Student, Tutor, Concept, Question, Response
- **Year**: 2025
- **Format**: JSONL
- **Size**: 188 dialogues
- **License**: Research
- **Citation**: Baker et al. (2025)
- **Notes**: LLM tutoring evaluation; knowledge tracing in AI tutors
- **URL**: <https://learninganalytics.upenn.edu/ryanbaker/>

### French Full-Length Fiction Coreference

**Rust ID**: `DatasetId::FrenchFullLengthFictionCoref`

Complete French novels spanning three centuries with character coreference.

- **Language**: fr
- **Domain**: fiction
- **Entity Types**: Character, Location, Organization
- **Year**: 2025
- **Format**: CoNLL
- **Annotation Scheme**: CoNLLCoref
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: French Fiction Team (2025)
- **Paper**: <https://arxiv.org/abs/2510.15594>
- **Notes**: Full novels with gender inference; tests long-document literary coref
- **URL**: <https://arxiv.org/html/2510.15594v1>

### Winograd Schema Challenge

**Rust ID**: `DatasetId::WinogradSchemaChallengeWSC`

Pronoun resolution requiring world knowledge. 273 sentence pairs.

- **Language**: en
- **Domain**: evaluation
- **Entity Types**: PER
- **Year**: 2012
- **Format**: XML
- **Size**: 273 sentence pairs
- **License**: Research
- **Citation**: Levesque et al. (2012)
- **Paper**: <https://aclanthology.org/N15-1117/>
- **Notes**: Commonsense reasoning benchmark; tests world knowledge in coreference
- **URL**: <https://cs.nyu.edu/~davise/papers/WinoPron/WSCollection.xml>

### TV Show Multilingual Coreference

**Rust ID**: `DatasetId::TVShowMultilingualCoref`

English TV show transcripts with projections to Chinese and Farsi.

- **Language**: multi
- **Domain**: dialogue
- **Entity Types**: Character, Location, Object
- **Year**: 2023
- **Format**: CoNLL
- **Annotation Scheme**: CoNLLCoref
- **License**: Research
- **Citation**: Khosla et al. (2023)
- **Paper**: <https://direct.mit.edu/tacl/article/doi/10.1162/tacl_a_00581>
- **Notes**: Cross-lingual projection via subtitles; multiparty TV dialogue
- **URL**: <https://direct.mit.edu/tacl/article/doi/10.1162/tacl_a_00581/117162>

### VisDial Coreference

**Rust ID**: `DatasetId::VisDialCoref`

Visual dialog with 120k images and 10-turn dialogs requiring visual coref.

- **Language**: en
- **Domain**: vision
- **Entity Types**: Object, Person, Location
- **Year**: 2017
- **Format**: JSONL
- **Size**: 120k images, 10-turn dialogs
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Das et al. (2017)
- **Paper**: <https://arxiv.org/abs/1611.08669>
- **Notes**: Visual coreference; grounding referents in images
- **URL**: <https://www.sciencedirect.com/science/article/pii/S266729522300082X>

### RISeC

**Rust ID**: `DatasetId::RISeC`

Procedural cooking text with temporal relations and manner descriptions.

- **Language**: en
- **Domain**: food
- **Entity Types**: Ingredient, Tool, Action, State, Time
- **Year**: 2024
- **Format**: Standoff
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: RISeC Team (2024)
- **Paper**: <https://arxiv.org/abs/2411.18157>
- **Notes**: Procedural coreference; tracks ingredient state through cooking steps
- **URL**: <https://arxiv.org/html/2411.18157v1>

### EFGC

**Rust ID**: `DatasetId::EFGC`

Cooking coreference segmented by tools, foods, and actions.

- **Language**: en
- **Domain**: food
- **Entity Types**: Food, Tool, Action
- **Year**: 2024
- **Format**: CoNLL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: EFGC Team (2024)
- **Paper**: <https://arxiv.org/abs/2411.18157>
- **Notes**: Entity flow graphs for cooking; tracks transformations
- **URL**: <https://arxiv.org/html/2411.18157v1>

### SPoRC

**Rust ID**: `DatasetId::SPoRC`

Structured Podcast Research Corpus. 1.1M episodes with host/guest extraction.

- **Language**: en
- **Domain**: speech
- **Entity Types**: Host, Guest, Organization, Topic
- **Year**: 2024
- **Format**: JSONL
- **Size**: 1.1M podcast episodes
- **License**: Research
- **Citation**: SPoRC Team (2024)
- **Paper**: <https://aclanthology.org/2025.acl-long.1222/>
- **Notes**: Speaker diarization; host/guest inference from transcripts
- **URL**: <https://arxiv.org/html/2411.07892v1>

### ARF (Artificial Relationships in Fiction)

**Rust ID**: `DatasetId::ARFFiction`

Synthetic RE dataset for literary texts. GPT-4o generated annotations.

- **Language**: en
- **Domain**: fiction
- **Entity Types**: Character, Location, Object, Event
- **Year**: 2025
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: ARF Team (2025)
- **Paper**: <https://aclanthology.org/2025.latechclfl-1.13/>
- **Notes**: Literary relationship extraction; synthetic from public domain fiction
- **URL**: <https://aclanthology.org/2025.latechclfl-1.13.pdf>

### CRAFT Corpus (Full Coref)

**Rust ID**: `DatasetId::CRAFTCorpusCoref`

Biomedical coref with ~30k relations. 23% span 500-12k words.

- **Language**: en
- **Domain**: biomedical
- **Entity Types**: Gene, Protein, Cell, Organism, Chemical
- **Year**: 2017
- **Format**: Standoff
- **Size**: 97 full-text PubMed articles, ~30k coref relations
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Cohen et al. (2017)
- **Paper**: <https://arxiv.org/html/2510.25087v1>
- **Notes**: Long-range dependencies; identity and appositive links; tests long-document coref
- **URL**: <https://github.com/UCDenver-ccp/CRAFT>

### Aerospace NER Dataset

**Rust ID**: `DatasetId::AerospaceNERDataset`

First open-source aerospace NER. 5 entity types for aviation knowledge graphs.

- **Language**: en
- **Domain**: aerospace
- **Entity Types**: Aircraft, Component, Manufacturer, Mission, System
- **Year**: 2023
- **Format**: CoNLL
- **License**: Research
- **Citation**: AIAA (2023)
- **Paper**: <https://arc.aiaa.org/doi/10.2514/1.I011251>
- **Notes**: Aviation product knowledge graphs; technical aerospace terminology
- **URL**: <https://arc.aiaa.org/doi/10.2514/1.I011251>

### Aviation Products NER

**Rust ID**: `DatasetId::AviationProductsNER`

Chinese aviation manufacturing corpus. Complex product entities.

- **Language**: zh
- **Domain**: aerospace
- **Entity Types**: Product, Component, Process, Material
- **Year**: 2022
- **Format**: BIO
- **License**: Research
- **Citation**: Cranfield (2022)
- **Notes**: Aviation manufacturing technical documents in Chinese
- **URL**: <https://dspace.lib.cranfield.ac.uk/server/api/core/bitstreams/a59ed640-4783-4ddb-871b-6fd8bd0e7400/content>

### VREN (Volleyball)

**Rust ID**: `DatasetId::VREN`

Volleyball rally descriptions for tactical statistics extraction.

- **Language**: en
- **Domain**: sports
- **Entity Types**: Player, Action, Position, Team, Score
- **Year**: 2024
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: VREN Team (2024)
- **Paper**: <https://arxiv.org/abs/2406.12252>
- **Notes**: Sports NLG; tactical action recognition from natural language
- **URL**: <https://arxiv.org/html/2406.12252v1>

### Fashion IQ

**Rust ID**: `DatasetId::FashionIQ`

77k fashion images with relative captions. 1000 attribute labels.

- **Language**: en
- **Domain**: fashion
- **Entity Types**: Texture, Fabric, Shape, Part, Style, Color
- **Year**: 2021
- **Format**: JSONL
- **Size**: 77k images, 1000 attribute labels
- **License**: Research
- **Citation**: Wu et al. (2021)
- **Paper**: <https://users.cs.utah.edu/~ziad/papers/cvpr_2021_fashion_iq.pdf>
- **Notes**: Dialog-based fashion retrieval; fine-grained attribute extraction
- **URL**: <https://github.com/XiaoxiaoGuo/fashion-iq>

### Natural Products RE

**Rust ID**: `DatasetId::NaturalProductsRE`

Relation extraction in underexplored biomedical domains. Diversity-sampled entities.

- **Language**: en
- **Domain**: biomedical
- **Entity Types**: NaturalProduct, Organism, Activity, Target
- **Year**: 2024
- **Format**: JSONL
- **License**: Research
- **Citation**: Hettiarachchi et al. (2024)
- **Paper**: <https://direct.mit.edu/coli/article/50/3/953/121178>
- **Notes**: LOTUS-derived NP dataset; synthetic data generation achieved F1=59.0
- **URL**: <https://direct.mit.edu/coli/article/50/3/953/121178>

### DrugProt

**Rust ID**: `DatasetId::DrugProtBioCreative`

Chemical-protein interactions from BioCreative VII challenge.

- **Language**: en
- **Domain**: biomedical
- **Entity Types**: Chemical, Gene, Protein
- **Year**: 2021
- **Format**: BRAT
- **License**: Research
- **Citation**: BioCreative VII (2021)
- **Paper**: <https://academic.oup.com/database/article/doi/10.1093/database/baac098/6833204>
- **Notes**: Drug-protein interaction classification; BioCreative shared task
- **URL**: <https://biocreative.bioinformatics.udel.edu/tasks/biocreative-vii/track-1/>

### MOF Dataset

**Rust ID**: `DatasetId::MOFDataset`

Metal-organic frameworks joint NER+RE. GPT-3/Llama extraction.

- **Language**: en
- **Domain**: materials
- **Entity Types**: MOF, Linker, Metal, Property, Application
- **Year**: 2024
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: MOF Team (2024)
- **Paper**: <https://pmc.ncbi.nlm.nih.gov/articles/PMC10869356/>
- **Notes**: Metal-organic framework literature; LLM-based extraction pipeline
- **URL**: <https://pmc.ncbi.nlm.nih.gov/articles/PMC10869356/>

### Solid-State Doping

**Rust ID**: `DatasetId::SolidStateDoping`

Impurity doping in materials. Joint NER+RE from literature.

- **Language**: en
- **Domain**: materials
- **Entity Types**: Host, Dopant, Property, Concentration, Method
- **Year**: 2024
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Doping Team (2024)
- **Paper**: <https://pmc.ncbi.nlm.nih.gov/articles/PMC10869356/>
- **Notes**: Semiconductor doping literature; tests materials science terminology
- **URL**: <https://pmc.ncbi.nlm.nih.gov/articles/PMC10869356/>

### AgriNER

**Rust ID**: `DatasetId::AgriNER`

Agricultural knowledge graph construction. 36 entity types, 9 relation types.

- **Language**: en
- **Domain**: agriculture
- **Entity Types**: Crop, Disease, Soil, Pathogen, Pesticide, Product
- **Year**: 2023
- **Format**: JSONL
- **License**: Research
- **Citation**: De et al. (2023)
- **Paper**: <https://2023.eswc-conferences.org/AgriNER/>
- **Notes**: Agricultural KG construction; covers crops, diseases, soil, pathogens
- **URL**: <https://2023.eswc-conferences.org/wp-content/uploads/2023/05/paper_De_2023_AgriNER.pdf>

### AGRONER

**Rust ID**: `DatasetId::AGRONER`

Unsupervised agricultural NER. Six major agricultural entity types.

- **Language**: en
- **Domain**: agriculture
- **Entity Types**: Disease, Soil, Pathogen, Pesticide, Crop, Product
- **Year**: 2023
- **Format**: BIO
- **License**: Research
- **Citation**: AGRONER Team (2023)
- **Paper**: <https://doi.org/10.1016/j.eswa.2023.121001>
- **Notes**: Unsupervised approach; no manual annotation required
- **URL**: <https://www.sciencedirect.com/science/article/abs/pii/S0957417423009429>

### AgMNER

**Rust ID**: `DatasetId::AgMNER`

Chinese multimodal agricultural NER. Text and speech combined.

- **Language**: zh
- **Domain**: agriculture
- **Entity Types**: Crop, Disease, Pest, Method
- **Year**: 2025
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: AgMNER Team (2025)
- **Paper**: <https://www.nature.com/articles/s41598-025-88874-9>
- **Notes**: Multimodal NER; combines text and speech for agricultural domain
- **URL**: <https://www.nature.com/articles/s41598-025-88874-9>

### Polish Coreference Corpus

**Rust ID**: `DatasetId::PolishCoreferenceCorpus`

Polish coreference resolution corpus. General domain Polish text.

- **Language**: pl
- **Domain**: general
- **Entity Types**: PER, ORG, LOC
- **Year**: 2015
- **Format**: Custom
- **Annotation Scheme**: Custom
- **License**: CC-BY-SA-4.0 (SPDX)
- **Citation**: Ogrodniczuk et al. (2015)
- **Notes**: Polish morphological complexity; rich inflection system
- **URL**: <http://zil.ipipan.waw.pl/PolishCoreferenceCorpus>

### Arabic Event Coreference

**Rust ID**: `DatasetId::ArabicEventCoref`

Arabic event coreference. Underexplored language for event coref.

- **Language**: ar
- **Domain**: news
- **Entity Types**: Event, Time, Location, Participant
- **Year**: 2024
- **Format**: CoNLL
- **Annotation Scheme**: CoNLLCoref
- **License**: Research
- **Citation**: Arabic Event Coref Team (2024)
- **Paper**: <https://dl.acm.org/doi/10.1145/3743047>
- **Notes**: Arabic event coreference; RTL script; underexplored language
- **URL**: <https://dl.acm.org/doi/10.1145/3743047>

### Hindi-English Social Media NER

**Rust ID**: `DatasetId::HindiEnglishSocialMediaNER`

Code-switched Hindi-English NER from social media.

- **Language**: hi-en
- **Domain**: social_media
- **Entity Types**: PER, LOC, ORG
- **Year**: 2018
- **Format**: CoNLL
- **License**: Research
- **Citation**: Hindi-English NER Team
- **Notes**: Code-switching between Hindi (Devanagari) and English; social media
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### astroBERT Corpus

**Rust ID**: `DatasetId::AstroBERTCorpus`

Domain-specific BERT trained on 395k astronomical papers.

- **Language**: en
- **Domain**: astronomy
- **Entity Types**: CelestialObject, Mission, Instrument, Phenomenon
- **Year**: 2023
- **Format**: Custom
- **Size**: 395,499 astronomical papers
- **License**: Research
- **Citation**: Grezes et al. (2023)
- **Paper**: <https://arxiv.org/abs/2310.17892>
- **Notes**: Domain-adapted BERT for astronomical entity extraction
- **URL**: <https://arxiv.org/html/2310.17892v2>

### Astronomical Telegram KEE

**Rust ID**: `DatasetId::AstronomicalTelegramKEE`

Event IDs, object names, telescope names from GCN Circulars.

- **Language**: en
- **Domain**: astronomy
- **Entity Types**: EventID, ObjectName, TelescopeName, Observatory
- **Year**: 2024
- **Format**: JSONL
- **License**: Research
- **Citation**: KEE Team (2024)
- **Paper**: <https://www.raa-journal.org/issues/all/2024/v24n6/202405/>
- **Notes**: LLM extraction from GCN Circulars; astronomical event reports
- **URL**: <https://www.raa-journal.org/issues/all/2024/v24n6/202405/>

### Saraga

**Rust ID**: `DatasetId::Saraga`

Indian Art Music dataset. Carnatic and Hindustani traditions.

- **Language**: multi
- **Domain**: music
- **Entity Types**: Raaga, Taala, Artist, Composition, Instrument
- **Year**: 2023
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Saraga Team (2023)
- **Paper**: <https://arxiv.org/abs/2309.16396>
- **Notes**: Indian classical music; Carnatic/Hindustani metadata extraction
- **URL**: <https://arxiv.org/pdf/2309.16396.pdf>

### MusicBrainz RE

**Rust ID**: `DatasetId::MusicBrainzRE`

Music metadata relations from Freebase/MusicBrainz. 116M instances.

- **Language**: en
- **Domain**: music
- **Entity Types**: Artist, Album, Track, Label, Genre
- **Year**: 2009
- **Format**: Custom
- **Size**: 116 million instances, 7,300 binary relations
- **License**: CC0
- **Citation**: Mintz et al. (2009)
- **Paper**: <https://web.stanford.edu/~jurafsky/mintz.pdf>
- **Notes**: Distant supervision from Freebase; music metadata relations
- **URL**: <https://web.stanford.edu/~jurafsky/mintz.pdf>

### DINAA

**Rust ID**: `DatasetId::DINAA`

Digital Index of North American Archaeology. Geospatial heritage data.

- **Language**: en
- **Domain**: archaeology
- **Entity Types**: Site, Artifact, Culture, Period, Location
- **Year**: 2015
- **Format**: Custom
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: DINAA Team
- **Notes**: North American archaeological sites; geospatial heritage preservation
- **URL**: <https://ux.opencontext.org/endangered-data-and-the-digital-index-of-north-american-archaeology-dinaa/>

### IMDb Semi-Structured RE

**Rust ID**: `DatasetId::IMDbSemiStructuredRE`

Distantly supervised extraction from structured web content.

- **Language**: en
- **Domain**: entertainment
- **Entity Types**: Movie, Person, Role, Date, Award
- **Year**: 2018
- **Format**: JSONL
- **License**: Research
- **Citation**: Lockard et al. (2018)
- **Paper**: <https://www.vldb.org/pvldb/vol11/p1084-lockard.pdf>
- **Notes**: Web table extraction; semi-structured movie database relations
- **URL**: <https://www.vldb.org/pvldb/vol11/p1084-lockard.pdf>

### ATIS Flight Booking

**Rust ID**: `DatasetId::ATISFlightBooking`

Slot-filling NER for flight booking intents. Classic NLU benchmark.

- **Language**: en
- **Domain**: travel
- **Entity Types**: FromCity, ToCity, DepartDate, ReturnDate, Airline, FlightNumber
- **Year**: 1990
- **Format**: BIO
- **License**: Research
- **Citation**: Hemphill et al. (1990)
- **Notes**: Classic slot-filling benchmark; spoken language understanding
- **URL**: <https://github.com/yvchen/JointSLU>

### Paleontology NER

**Rust ID**: `DatasetId::PaleontologyNER`

Dinosaurs, mammals, and river ecosystems entity retrieval.

- **Language**: en
- **Domain**: paleontology
- **Entity Types**: Taxon, Location, TimePeriod, Formation, Specimen
- **Year**: 2023
- **Format**: CoNLL
- **License**: Research
- **Citation**: Paleo NER Team (2023)
- **Paper**: <https://aclanthology.org/2023.findings-emnlp.218/>
- **Notes**: Paleontological literature; fossil taxa and geological formations
- **URL**: <https://aclanthology.org/anthology-files/anthology-files/pdf/findings/2023.findings-emnlp.218v1.pdf>

### Water Resource NER

**Rust ID**: `DatasetId::WaterResourceNER`

Domain-adaptive NER for AI-driven water resource management.

- **Language**: en
- **Domain**: environment
- **Entity Types**: WaterBody, Infrastructure, Pollutant, Measurement, Policy
- **Year**: 2025
- **Format**: BIO
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Water NER Team (2025)
- **Paper**: <https://www.frontiersin.org/articles/10.3389/fenvs.2025.1558317/>
- **Notes**: Water management domain; infrastructure and policy entities
- **URL**: <https://www.frontiersin.org/journals/environmental-science/articles/10.3389/fenvs.2025.1558317/pdf>

### MalwareTextDB

**Rust ID**: `DatasetId::MalwareTextDB`

Annotated malware articles for cybersecurity NER.

- **Language**: en
- **Domain**: cybersecurity
- **Entity Types**: Malware, Vulnerability, Tool, ThreatActor, IOC
- **Year**: 2017
- **Format**: BRAT
- **License**: Research
- **Citation**: MalwareTextDB Team
- **Notes**: Security bulletin extraction; malware family identification
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### SEC-filings

**Rust ID**: `DatasetId::SECFilingsNER`

Finance domain NER from SEC filing documents.

- **Language**: en
- **Domain**: finance
- **Entity Types**: Company, Person, Money, Date, Percentage
- **Year**: 2018
- **Format**: CoNLL
- **License**: CC-BY-3.0 (SPDX)
- **Citation**: SEC-filings Team
- **Notes**: Financial documents; SEC 10-K and 10-Q filings
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### AnEM

**Rust ID**: `DatasetId::AnEM`

Anatomical entity mentions corpus. Anatomy terms in biomedical text.

- **Language**: en
- **Domain**: biomedical
- **Entity Types**: AnatomicalStructure, Organ, Tissue, Cell, OrganismSubdivision
- **Year**: 2012
- **Format**: Standoff
- **License**: CC-BY-SA-3.0 (SPDX)
- **Citation**: Ohta et al. (2012)
- **Notes**: Anatomical entity corpus; fine-grained anatomy typing
- **URL**: <http://www.nactem.ac.uk/anatomy/>

### RecipeDB Annotated

**Rust ID**: `DatasetId::RecipeDBAnnotated`

88k ingredient phrases via clustering-based sampling with Stanford NER.

- **Language**: en
- **Domain**: food
- **Entity Types**: Ingredient, Quantity, Unit, Preparation
- **Year**: 2024
- **Format**: JSONL
- **Size**: 88,526 ingredient phrases
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: RecipeDB Team (2024)
- **Paper**: <https://aclanthology.org/2024.lrec-main.406/>
- **Notes**: Clustering-based annotation; Stanford NER pipeline
- **URL**: <https://aclanthology.org/2024.lrec-main.406/>

### Ritter Twitter NER

**Rust ID**: `DatasetId::RitterTwitterNER`

Twitter NER dataset with diverse entity types from tweets.

- **Language**: en
- **Domain**: social_media
- **Entity Types**: PER, LOC, ORG, PRODUCT, FACILITY, BAND, SPORTSTEAM
- **Year**: 2011
- **Format**: CoNLL
- **License**: Research
- **Citation**: Ritter et al. (2011)
- **Paper**: <https://aclanthology.org/D11-1141/>
- **Notes**: Early Twitter NER; 10 entity types including bands and sports teams
- **URL**: <https://github.com/aritter/twitter_nlp>

### Music-NER

**Rust ID**: `DatasetId::MusicNER`

Music domain entities. Artists, albums, songs, genres.

- **Language**: en
- **Domain**: music
- **Entity Types**: Artist, Album, Song, Genre, Instrument, Label
- **Year**: 2020
- **Format**: CoNLL
- **License**: MIT (SPDX)
- **Citation**: Music-NER Team
- **Notes**: Music domain NER; includes record labels and instrument types
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### 500 Tutoring Sessions

**Rust ID**: `DatasetId::TutoringSessionsAlgebra`

32k utterances from elementary algebra/physics tutoring. Mode identification.

- **Language**: en
- **Domain**: education
- **Entity Types**: Student, Tutor, Concept, Problem
- **Year**: 2016
- **Format**: Custom
- **Size**: 500 sessions, 32,368 utterances
- **License**: Research
- **Citation**: Boyer et al. (2016)
- **Paper**: <https://aclanthology.org/C16-1188/>
- **Notes**: Tutoring mode identification; algebra and physics domains
- **URL**: <https://aclanthology.org/C16-1188.pdf>

### GNER

**Rust ID**: `DatasetId::GNERGeoscience`

Chinese geological entities from geoscience survey reports.

- **Language**: zh
- **Domain**: geology
- **Entity Types**: Rock, Mineral, Stratum, Age, Location
- **Year**: 2019
- **Format**: BIO
- **License**: Research
- **Citation**: GNER Team (2019)
- **Paper**: <https://doi.org/10.1029/2019EA000610>
- **Notes**: Chinese geoscience reports; geological survey terminology
- **URL**: <https://agupubs.onlinelibrary.wiley.com/doi/abs/10.1029/2019EA000610>

### Four Regions Geology NER

**Rust ID**: `DatasetId::FourRegionsGeologyNER`

Regional geological surveys with 6 typical geological categories.

- **Language**: zh
- **Domain**: geology
- **Entity Types**: Rock, Mineral, Stratum, Structure, Age, Location
- **Year**: 2020
- **Format**: BIO
- **License**: Research
- **Citation**: Four Regions Team
- **Notes**: Regional Chinese geological surveys; multiple survey regions
- **URL**: <https://www.geodoi.ac.cn/WebEn/down.aspx?ID=1873>

### MSP-Podcast

**Rust ID**: `DatasetId::MSPPodcast`

100k+ English podcast episodes with multimodal annotations.

- **Language**: en
- **Domain**: speech
- **Entity Types**: Speaker, Topic, Emotion, Sentiment
- **Year**: 2019
- **Format**: Custom
- **Size**: 100,000+ podcast episodes
- **License**: Research
- **Citation**: Lotfian & Busso (2019)
- **Notes**: Multimodal podcast annotations; emotion and sentiment
- **URL**: <https://ecs.utdallas.edu/research/researchlabs/msp-lab/MSP-Podcast.html>

### Spotify Podcasts Dataset

**Rust ID**: `DatasetId::SpotifyPodcastsDataset`

Professional and amateur podcast episodes with transcriptions.

- **Language**: en
- **Domain**: speech
- **Entity Types**: Host, Guest, Topic, Advertisement
- **Year**: 2023
- **Format**: JSONL
- **License**: Research
- **Citation**: Spotify Research (2023)
- **Paper**: <https://www.isca-archive.org/interspeech_2023/kotey23_interspeech.html>
- **Notes**: Professional and amateur podcasts; varied audio quality
- **URL**: <https://www.isca-archive.org/interspeech_2023/kotey23_interspeech.pdf>

### Sports NER

**Rust ID**: `DatasetId::SportsNERGeneral`

Player names, team names, event specifics from sports texts.

- **Language**: en
- **Domain**: sports
- **Entity Types**: Player, Team, Event, Venue, Score, Date
- **Year**: 2024
- **Format**: CoNLL
- **License**: Research
- **Citation**: Sports NER Team (2024)
- **Paper**: <https://arxiv.org/abs/2406.12252>
- **Notes**: General sports domain; player and team tracking
- **URL**: <https://arxiv.org/html/2406.12252v1>

### Esports NER

**Rust ID**: `DatasetId::EsportsNER`

Esports entity recognition. Pro players, teams, tournaments, games.

- **Language**: en
- **Domain**: gaming
- **Entity Types**: Player, Team, Tournament, Game, Champion, Map
- **Year**: 2024
- **Format**: CoNLL
- **License**: Research
- **Citation**: Esports NER Team (2024)
- **Notes**: Competitive gaming; League of Legends, CS:GO, Dota 2 terminology
- **URL**: <https://arxiv.org/html/2406.12252v1>

### DeepFashion2

**Rust ID**: `DatasetId::DeepFashion2`

Comprehensive fashion dataset. 491k images, 801k clothing items.

- **Language**: en
- **Domain**: fashion
- **Entity Types**: Category, Style, Color, Pattern, Landmark
- **Year**: 2019
- **Format**: JSONL
- **Size**: 491k images, 801k clothing items, 13 categories
- **License**: Research
- **Citation**: Ge et al. (2019)
- **Paper**: <https://arxiv.org/abs/1901.07973>
- **Notes**: Dense landmarks; cross-domain pose variation
- **URL**: <https://github.com/switchablenorms/DeepFashion2>

### Construction NER

**Rust ID**: `DatasetId::ConstructionNER`

Construction industry entities. Materials, equipment, processes.

- **Language**: en
- **Domain**: construction
- **Entity Types**: Material, Equipment, Process, Measurement, Location
- **Year**: 2021
- **Format**: BIO
- **License**: Research
- **Citation**: Construction NER Team (2021)
- **Notes**: Construction domain; building materials and heavy equipment
- **URL**: <https://www.sciencedirect.com/science/article/pii/S0926580520309481>

### PharmaNER

**Rust ID**: `DatasetId::PharmaNER`

Pharmaceutical named entity recognition. Drug names, dosages, routes.

- **Language**: en
- **Domain**: biomedical
- **Entity Types**: Drug, Dosage, Route, Frequency, Indication
- **Year**: 2019
- **Format**: BIO
- **License**: Research
- **Citation**: PharmaNER Team
- **Notes**: Pharmaceutical domain; prescription and OTC drug extraction
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Product Review NER

**Rust ID**: `DatasetId::ProductReviewNER`

E-commerce product reviews with aspect and sentiment entities.

- **Language**: en
- **Domain**: ecommerce
- **Entity Types**: Aspect, Opinion, Product, Feature, Sentiment
- **Year**: 2014
- **Format**: XML
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: SemEval 2014
- **Paper**: <https://aclanthology.org/S14-2004/>
- **Notes**: Aspect-based sentiment; product feature extraction
- **URL**: <https://www.aclweb.org/anthology/S14-2004/>

### Real Estate NER

**Rust ID**: `DatasetId::RealEstateNER`

Property listings entity extraction. Addresses, prices, features.

- **Language**: en
- **Domain**: real_estate
- **Entity Types**: Address, Price, Size, Rooms, Amenity, PropertyType
- **Year**: 2020
- **Format**: CoNLL
- **License**: Research
- **Citation**: Real Estate NER Team
- **Notes**: Property listing domain; residential and commercial
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Automotive NER

**Rust ID**: `DatasetId::AutomotiveNER`

Vehicle and automotive entities. Makes, models, parts, specs.

- **Language**: en
- **Domain**: automotive
- **Entity Types**: Make, Model, Part, Specification, Year, Price
- **Year**: 2021
- **Format**: CoNLL
- **License**: Research
- **Citation**: Automotive NER Team
- **Notes**: Automotive domain; vehicle specifications and parts
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Tourism NER

**Rust ID**: `DatasetId::TourismNER`

Tourism and travel entities. Attractions, hotels, restaurants.

- **Language**: en
- **Domain**: tourism
- **Entity Types**: Attraction, Hotel, Restaurant, City, Activity, Price
- **Year**: 2019
- **Format**: CoNLL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Tourism NER Team
- **Notes**: Travel domain; tourist attractions and accommodations
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Energy NER

**Rust ID**: `DatasetId::EnergyNER`

Energy sector entities. Power plants, fuels, grid infrastructure.

- **Language**: en
- **Domain**: energy
- **Entity Types**: PowerPlant, Fuel, Grid, Capacity, Company, Location
- **Year**: 2020
- **Format**: BIO
- **License**: Research
- **Citation**: Energy NER Team
- **Notes**: Energy sector; renewable and fossil fuel infrastructure
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Insurance NER

**Rust ID**: `DatasetId::InsuranceNER`

Insurance domain entities. Policies, claims, coverages.

- **Language**: en
- **Domain**: insurance
- **Entity Types**: Policy, Claim, Coverage, Premium, Deductible, Beneficiary
- **Year**: 2021
- **Format**: JSONL
- **License**: Research
- **Citation**: Insurance NER Team
- **Notes**: Insurance domain; policy and claims extraction
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Logistics NER

**Rust ID**: `DatasetId::LogisticsNER`

Supply chain and logistics entities. Shipments, warehouses, routes.

- **Language**: en
- **Domain**: logistics
- **Entity Types**: Shipment, Warehouse, Route, Carrier, TrackingNumber, Date
- **Year**: 2020
- **Format**: CoNLL
- **License**: Research
- **Citation**: Logistics NER Team
- **Notes**: Supply chain domain; shipping and warehousing
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Resume NER

**Rust ID**: `DatasetId::ResumeNER`

Resume/CV entity extraction. Skills, experience, education.

- **Language**: en
- **Domain**: hr
- **Entity Types**: Skill, Company, Degree, University, Date, Location
- **Year**: 2018
- **Format**: JSONL
- **License**: CC0
- **Citation**: DataTurks
- **Notes**: Resume parsing; skill and experience extraction
- **URL**: <https://www.kaggle.com/datasets/dataturks/resume-entities-for-ner>

### Job Posting NER

**Rust ID**: `DatasetId::JobPostingNER`

Job posting entity extraction. Requirements, benefits, qualifications.

- **Language**: en
- **Domain**: hr
- **Entity Types**: JobTitle, Skill, Salary, Location, Company, Benefit
- **Year**: 2020
- **Format**: CoNLL
- **License**: Research
- **Citation**: Job Posting NER Team
- **Notes**: Job listing domain; requirement and qualification extraction
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Healthcare Admin NER

**Rust ID**: `DatasetId::HealthcareAdminNER`

Healthcare administration entities. Procedures, billing codes, facilities.

- **Language**: en
- **Domain**: healthcare
- **Entity Types**: Procedure, BillingCode, Facility, Provider, Insurance
- **Year**: 2021
- **Format**: BIO
- **License**: Research
- **Citation**: Healthcare Admin Team
- **Notes**: Healthcare administration; billing and coding
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Telecom NER

**Rust ID**: `DatasetId::TelecomNER`

Telecommunications entities. Networks, devices, protocols.

- **Language**: en
- **Domain**: telecom
- **Entity Types**: Network, Device, Protocol, Carrier, Plan, Speed
- **Year**: 2020
- **Format**: CoNLL
- **License**: Research
- **Citation**: Telecom NER Team
- **Notes**: Telecommunications domain; network and service extraction
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Weather NER

**Rust ID**: `DatasetId::WeatherNER`

Weather and climate entities. Events, measurements, locations.

- **Language**: en
- **Domain**: weather
- **Entity Types**: WeatherEvent, Temperature, Precipitation, Location, Date, Wind
- **Year**: 2021
- **Format**: BIO
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Weather NER Team
- **Notes**: Meteorological domain; weather event extraction
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Manufacturing NER

**Rust ID**: `DatasetId::ManufacturingNER`

Manufacturing entities. Parts, processes, machines, defects.

- **Language**: en
- **Domain**: manufacturing
- **Entity Types**: Part, Process, Machine, Defect, Material, Measurement
- **Year**: 2021
- **Format**: BIO
- **License**: Research
- **Citation**: Manufacturing NER Team
- **Notes**: Industrial manufacturing; quality control and process
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Retail Inventory NER

**Rust ID**: `DatasetId::RetailInventoryNER`

Retail inventory entities. SKUs, quantities, locations, prices.

- **Language**: en
- **Domain**: retail
- **Entity Types**: SKU, Quantity, Location, Price, Category, Supplier
- **Year**: 2020
- **Format**: JSONL
- **License**: Research
- **Citation**: Retail NER Team
- **Notes**: Inventory management; stock and supplier tracking
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Crop Disease NER

**Rust ID**: `DatasetId::CropDiseaseNER`

Crop disease identification. Symptoms, pathogens, treatments.

- **Language**: en
- **Domain**: agriculture
- **Entity Types**: Disease, Symptom, Pathogen, Treatment, Crop, Stage
- **Year**: 2022
- **Format**: BIO
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Crop Disease Team
- **Notes**: Plant pathology; disease symptom and treatment extraction
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Wine NER

**Rust ID**: `DatasetId::WineNER`

Wine domain entities. Varietals, regions, vintages, tasting notes.

- **Language**: en
- **Domain**: food
- **Entity Types**: Varietal, Region, Vintage, Producer, TastingNote, Price
- **Year**: 2019
- **Format**: CoNLL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Wine NER Team
- **Notes**: Wine domain; sommelier terminology and tasting vocabulary
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Veterinary NER

**Rust ID**: `DatasetId::VeterinaryNER`

Veterinary medicine entities. Animals, conditions, treatments.

- **Language**: en
- **Domain**: veterinary
- **Entity Types**: Animal, Breed, Condition, Treatment, Medication, Symptom
- **Year**: 2021
- **Format**: BIO
- **License**: Research
- **Citation**: Veterinary NER Team
- **Notes**: Veterinary medicine; pet health and treatment
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Photography NER

**Rust ID**: `DatasetId::PhotographyNER`

Photography entities. Cameras, lenses, settings, techniques.

- **Language**: en
- **Domain**: photography
- **Entity Types**: Camera, Lens, Aperture, ShutterSpeed, ISO, Technique
- **Year**: 2020
- **Format**: CoNLL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Photography NER Team
- **Notes**: Photography domain; camera gear and technique extraction
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Genealogy NER

**Rust ID**: `DatasetId::GenealogyNER`

Genealogical records entities. Names, relationships, dates, locations.

- **Language**: en
- **Domain**: genealogy
- **Entity Types**: Person, Relationship, BirthDate, DeathDate, Location, Occupation
- **Year**: 2021
- **Format**: Custom
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Genealogy NER Team
- **Notes**: Historical records; family history extraction
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Board Game NER

**Rust ID**: `DatasetId::BoardGameNER`

Board game entities. Games, mechanics, components, designers.

- **Language**: en
- **Domain**: gaming
- **Entity Types**: Game, Mechanic, Component, Designer, Publisher, PlayerCount
- **Year**: 2022
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: BoardGameGeek
- **Notes**: Board game domain; BGG taxonomy and mechanics
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Gardening NER

**Rust ID**: `DatasetId::GardeningNER`

Gardening entities. Plants, soil, seasons, techniques.

- **Language**: en
- **Domain**: gardening
- **Entity Types**: Plant, Soil, Season, Technique, Tool, Pest
- **Year**: 2021
- **Format**: CoNLL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Gardening NER Team
- **Notes**: Horticulture domain; plant care and cultivation
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Brewing NER

**Rust ID**: `DatasetId::BrewingNER`

Craft brewing entities. Ingredients, processes, styles, equipment.

- **Language**: en
- **Domain**: food
- **Entity Types**: Ingredient, Process, Style, Equipment, ABV, IBU
- **Year**: 2020
- **Format**: CoNLL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Brewing NER Team
- **Notes**: Craft beer domain; brewing process and style vocabulary
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Knitting NER

**Rust ID**: `DatasetId::KnittingNER`

Knitting and crafts entities. Patterns, yarns, stitches, tools.

- **Language**: en
- **Domain**: crafts
- **Entity Types**: Pattern, Yarn, Stitch, Tool, Size, Technique
- **Year**: 2021
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Ravelry
- **Notes**: Fiber arts domain; knitting pattern terminology
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Fitness NER

**Rust ID**: `DatasetId::FitnessNER`

Fitness entities. Exercises, muscles, equipment, routines.

- **Language**: en
- **Domain**: fitness
- **Entity Types**: Exercise, Muscle, Equipment, Sets, Reps, Duration
- **Year**: 2021
- **Format**: CoNLL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Fitness NER Team
- **Notes**: Exercise domain; workout routine extraction
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Astrology NER

**Rust ID**: `DatasetId::AstrologyNER`

Astrological entities. Signs, planets, houses, aspects.

- **Language**: en
- **Domain**: astrology
- **Entity Types**: Sign, Planet, House, Aspect, Transit, Date
- **Year**: 2021
- **Format**: CoNLL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Astrology NER Team
- **Notes**: Astrological terminology; horoscope interpretation
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Tattoo NER

**Rust ID**: `DatasetId::TattooNER`

Tattoo entities. Styles, placements, artists, designs.

- **Language**: en
- **Domain**: art
- **Entity Types**: Style, Placement, Artist, Design, Color, Size
- **Year**: 2022
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Tattoo NER Team
- **Notes**: Body art domain; tattoo style and placement vocabulary
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Fragrance NER

**Rust ID**: `DatasetId::FragranceNER`

Perfume entities. Notes, accords, houses, concentrations.

- **Language**: en
- **Domain**: fragrance
- **Entity Types**: Note, Accord, House, Concentration, Season, Longevity
- **Year**: 2021
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Fragrantica
- **Notes**: Perfumery domain; scent pyramid and accord terminology
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Chess NER

**Rust ID**: `DatasetId::ChessNER`

Chess entities. Openings, players, tournaments, moves.

- **Language**: en
- **Domain**: gaming
- **Entity Types**: Opening, Player, Tournament, Move, ELO, TimeControl
- **Year**: 2022
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Lichess/Chess.com
- **Notes**: Chess domain; opening theory and tournament extraction
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Cocktail NER

**Rust ID**: `DatasetId::CocktailNER`

Cocktail entities. Ingredients, techniques, glassware, garnishes.

- **Language**: en
- **Domain**: food
- **Entity Types**: Spirit, Mixer, Technique, Glassware, Garnish, Measurement
- **Year**: 2020
- **Format**: CoNLL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Cocktail NER Team
- **Notes**: Mixology domain; bartending vocabulary and techniques
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Antiques NER

**Rust ID**: `DatasetId::AntiquesNER`

Antiques entities. Periods, styles, materials, makers.

- **Language**: en
- **Domain**: antiques
- **Entity Types**: Period, Style, Material, Maker, Provenance, Condition
- **Year**: 2021
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Antiques NER Team
- **Notes**: Antiques domain; period furniture and collectibles
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Maritime NER

**Rust ID**: `DatasetId::MaritimeNER`

Maritime entities. Vessels, ports, routes, cargo.

- **Language**: en
- **Domain**: maritime
- **Entity Types**: Vessel, Port, Route, Cargo, Flag, IMONumber
- **Year**: 2021
- **Format**: CoNLL
- **License**: Research
- **Citation**: Maritime NER Team
- **Notes**: Shipping domain; vessel tracking and maritime logistics
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Equestrian NER

**Rust ID**: `DatasetId::EquestrianNER`

Equestrian entities. Horses, breeds, disciplines, tack.

- **Language**: en
- **Domain**: equestrian
- **Entity Types**: Horse, Breed, Discipline, Tack, Rider, Competition
- **Year**: 2021
- **Format**: CoNLL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Equestrian NER Team
- **Notes**: Horse sports domain; dressage and jumping terminology
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Woodworking NER

**Rust ID**: `DatasetId::WoodworkingNER`

Woodworking entities. Tools, joints, wood types, finishes.

- **Language**: en
- **Domain**: crafts
- **Entity Types**: Tool, Joint, WoodType, Finish, Technique, Measurement
- **Year**: 2021
- **Format**: CoNLL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Woodworking NER Team
- **Notes**: Carpentry domain; joinery and finishing vocabulary
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Birdwatching NER

**Rust ID**: `DatasetId::BirdwatchingNER`

Birdwatching entities. Species, habitats, behaviors, locations.

- **Language**: en
- **Domain**: wildlife
- **Entity Types**: Species, Family, Habitat, Behavior, Location, Season
- **Year**: 2022
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: eBird/Cornell Lab
- **Notes**: Ornithology domain; bird identification and behavior
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Numismatics NER

**Rust ID**: `DatasetId::NumismaticsNER`

Coin collecting entities. Denominations, mints, grades, errors.

- **Language**: en
- **Domain**: numismatics
- **Entity Types**: Denomination, Mint, Grade, Error, Year, Metal
- **Year**: 2021
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: PCGS/NGC
- **Notes**: Coin collecting; grading and mint terminology
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Philately NER

**Rust ID**: `DatasetId::PhilatelyNER`

Stamp collecting entities. Issues, perforations, watermarks, varieties.

- **Language**: en
- **Domain**: philately
- **Entity Types**: Issue, Perforation, Watermark, Variety, Country, Year
- **Year**: 2021
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Scott Catalogue
- **Notes**: Stamp collecting; philatelic terminology
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Scuba NER

**Rust ID**: `DatasetId::ScubaNER`

Scuba diving entities. Equipment, sites, certifications, marine life.

- **Language**: en
- **Domain**: scuba
- **Entity Types**: Equipment, DiveSite, Certification, MarineLife, Depth, Visibility
- **Year**: 2021
- **Format**: CoNLL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: PADI/SSI
- **Notes**: Recreational diving; dive site and equipment extraction
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Theme Park NER

**Rust ID**: `DatasetId::ThemeParkNER`

Theme park entities. Rides, parks, manufacturers, statistics.

- **Language**: en
- **Domain**: entertainment
- **Entity Types**: Ride, Park, Manufacturer, Height, Speed, Type
- **Year**: 2022
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: RCDB
- **Notes**: Amusement park domain; roller coaster specifications
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Origami NER

**Rust ID**: `DatasetId::OrigamiNER`

Origami entities. Folds, bases, models, paper types.

- **Language**: en
- **Domain**: crafts
- **Entity Types**: Fold, Base, Model, PaperType, Designer, Difficulty
- **Year**: 2021
- **Format**: CoNLL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Origami NER Team
- **Notes**: Paper folding domain; fold terminology and model names
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Anime/Manga NER

**Rust ID**: `DatasetId::AnimeMangaNER`

Anime and manga entities. Titles, characters, studios, genres.

- **Language**: multi
- **Domain**: entertainment
- **Entity Types**: Title, Character, Studio, Genre, Author, Year
- **Year**: 2022
- **Format**: JSONL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: MyAnimeList/AniDB
- **Notes**: Japanese animation; includes romanized and Japanese names
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Crypto NER

**Rust ID**: `DatasetId::CryptoNER`

Cryptocurrency entities. Tokens, wallets, exchanges, protocols.

- **Language**: en
- **Domain**: crypto
- **Entity Types**: Token, Wallet, Exchange, Protocol, Price, Address
- **Year**: 2022
- **Format**: CoNLL
- **License**: Research
- **Citation**: Crypto NER Team
- **Notes**: Blockchain domain; DeFi and NFT terminology
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Telenovela NER

**Rust ID**: `DatasetId::TelenovelaNER`

Spanish-language soap opera entities. Characters, relationships, plots.

- **Language**: es
- **Domain**: entertainment
- **Entity Types**: Character, Relationship, PlotPoint, Actor, Network
- **Year**: 2021
- **Format**: CoNLL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Telenovela NER Team
- **Notes**: Spanish soap operas; melodrama terminology
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Tarot NER

**Rust ID**: `DatasetId::TarotNER`

Tarot entities. Cards, spreads, meanings, suits.

- **Language**: en
- **Domain**: divination
- **Entity Types**: Card, Spread, Meaning, Suit, Position, Reversal
- **Year**: 2021
- **Format**: CoNLL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Tarot NER Team
- **Notes**: Tarot reading; card interpretation vocabulary
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

### Beekeeping NER

**Rust ID**: `DatasetId::BeekeepingNER`

Apiculture entities. Equipment, bee types, diseases, products.

- **Language**: en
- **Domain**: agriculture
- **Entity Types**: Equipment, BeeType, Disease, Product, Season, Technique
- **Year**: 2021
- **Format**: CoNLL
- **License**: CC-BY-4.0 (SPDX)
- **Citation**: Beekeeping NER Team
- **Notes**: Apiculture domain; hive management vocabulary
- **URL**: <https://github.com/juand-r/entity-recognition-datasets>

