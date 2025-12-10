// Generated loader stubs from datasets_generated.toml
// Existing variants: 188
// Registry datasets: 184
// Missing variants: 122

// === ENUM VARIANTS ===
// Add these to `pub enum DatasetId {`

    /// ACE 2005 relation extraction component. 7 entity types, 6 re
    ACE05RE,
    /// Adverse Drug Reaction corpus with discontinuous mentions. Pa
    ADRDiscontinuous,
    /// Primary entity linking benchmark linking CoNLL-2003 mentions
    AIDACoNLL,
    /// Newswire entity linking dataset from AQUAINT corpus. Wikiped
    AQUAINT,
    /// Large-scale Chinese agricultural NER. 66k samples, ~207k ent
    AgCNER,
    /// Discourse anaphora accessibility evaluation. Tests non-NP an
    AnaphoraAccessibility,
    /// Cyber threat intelligence NER with MITRE ATT&CK linking. 400
    AnnoCTR,
    /// Bioacoustics benchmark beyond species classification. Natura
    BEANSZero,
    /// Biomedical Entity Linking Benchmark unifying 11 corpora acro
    BELB,
    /// Named entity recognition for Basque (Euskara). Language isol
    BasqueNER,
    /// Instruction-tuned biomedical NER benchmark. Evaluates genera
    BioNERLLaMA,
    /// Bias in Open-ended Language Generation Dataset. Wikipedia-ba
    BoldBias,
    /// Full-novel coreference with automatic silver and manual gold
    BookCoref,
    /// Coreference annotations on book chapters from BookSum. Long 
    BookSumCoref,
    /// Code-Switching Workshop shared task. English-Spanish Twitter
    CALCS2018,
    /// Burgundian medieval Latin charters NER. 9th-14th century dip
    CBMACharters,
    /// Chemical compound and drug name recognition in scientific te
    CHEMDNER,
    /// Universal Anaphora bridging annotations. One of the largest 
    CODICRACBridging,
    /// Chinese nested named entity recognition. Multiple levels of 
    ChineseNestedNER,
    /// Sentence-level relation extraction from CoNLL-2004. Clean, s
    CoNLL04RE,
    /// Conversational Question Answering. Multi-turn QA requiring e
    CoQAEntities,
    /// Code understanding benchmark. Function documentation and cod
    CodeSearchNet,
    /// Sahidic Coptic with multi-layer annotation. ~50k tokens.
    CopticScriptorium,
    /// Cross-domain relation extraction across 6 domains. Tests RE 
    CrossRE,
    /// Cross-lingual adversarial NER evaluation. Tests multilingual
    CrossWeigh,
    /// Crowdsourced stereotype pairs benchmark. 9 bias categories f
    CrowSPairs,
    /// Dialogue-based relation extraction. Multi-turn conversations
    DialogRE,
    /// 1,283 musical scores with harmonic annotations. String quart
    DistantListeningCorpus,
    /// Archaeological excavation reports from DANS archive. 31k ann
    DutchArchaeology,
    /// Gold-standard multi-genre Polish NER+EL. Includes fiction, p
    ELGold,
    /// Legal NER from SEC EDGAR filings. 52 documents with financia
    ENERSec,
    /// NER for US SEC EDGAR filings. 52 documents, 400k+ tokens wit
    ENer,
    /// Entity linking for occupational skills to ESCO taxonomy. Job
    ESCOSkillsEL,
    /// Enzyme chemistry relation extraction. Links enzymes, substra
    EnzChemRED,
    /// Multi-perspective concept drift detection on event knowledge
    EventKGDrift,
    /// Fiction Adapted BERT for Literary Entities. DeBERTa-based NE
    FABLE,
    /// Cross-document event coreference for football matches. Requi
    FCC,
    /// Food ingredient NER from AllRecipes. 182k sentences with ing
    FINER,
    /// Financial NER with 139 fine-grained entity types. SEC 10-K/1
    FiNER139,
    /// Financial NER from FinBen benchmark. Entity extraction from 
    FinBenNER,
    /// Geoparsing benchmark from web news. Toponyms with geocoding 
    GeoWebNews,
    /// German discontinuous NER from GermEval 2014. Non-contiguous 
    GermEvalDiscontinuous,
    /// Hindi-English code-mixed social media NER. Roman script Hind
    HinglishNER,
    /// Romanian historical newspaper NER. First Romanian historical
    HistNERo,
    /// Clinical concept extraction and assertion classification. Fo
    I2B22010,
    /// Clinical temporal relations challenge. Events, TIMEX3, and T
    I2B2Temporal,
    /// Indian languages NER covering 11 Indian languages. Low-resou
    IndicNER,
    /// Interlingue (Occidental) Wikipedia text corpus. Internationa
    InterlingueWikipedia,
    /// Short, highly ambiguous entity linking snippets. Tests disam
    KORE50,
    /// Language ID dataset with 11 constructed languages. 14.2M sen
    KlingonEffectLID,
    /// Large-scale multilingual conflict event corpus. 39k events a
    LEMONADE,
    /// Local-Global Lexicon for toponym disambiguation. News articl
    LGL,
    /// Biblical Hebrew NER and coreference annotation.
    LT4HALA,
    /// Universal Dependencies for Latin. Classical through Medieval
    LatinUD,
    /// Event coreference in long legal documents. Long-distance cro
    LegalCore,
    /// Legal NER from LexGLUE benchmark. Legal entity extraction fr
    LexGLUENER,
    /// Lojban-English sentence pairs from Tatoeba. Logical language
    LojbanTatoeba,
    /// Long-document NER benchmark. Tests entity recognition across
    LongDocNER,
    /// Biomedical NER corpus with extensive coverage. Used with RoB
    MACCROBAT,
    /// Multi-Axis Temporal Relations. Cleaner, more consistent even
    MATRES,
    /// Multilingual news corpus with within- and cross-document eve
    MEANTIME,
    /// Multilingual Entity Linking of Occupations. 48 datasets acro
    MELO,
    /// Historical long-tail entity linking benchmark. Tests LLM beh
    MHERCL,
    /// Multimodal NER with Multiple Images. Social media posts with
    MNERMI,
    /// Small news article entity linking dataset. Commonly used for
    MSNBCEL,
    /// Named entity recognition for Te Reo Māori. New Zealand indig
    MaoriNER,
    /// Terminology and definition extraction from mathematical text
    MathEntities,
    /// Large-scale biomedical concept mentions mapped to UMLS. PubM
    MedMentions,
    /// Multilingual medieval charter NER. Latin, French, Spanish fr
    MedievalCharterNER,
    /// MCQ-format coreference for LLMs from LitBank and FantasyCore
    MentionResolutionLLM,
    /// Long biomedical document NER. Full-text articles vs abstract
    MultiBioNERLong,
    /// Multi-domain task-oriented dialogue with slot/entity annotat
    MultiWOZNER,
    /// Named Clinical Entity Recognition Benchmark. Multi-dataset c
    NCERB,
    /// New York Times distant supervision RE. 24 Freebase relations
    NYT10,
    /// Nigerian Pidgin NER corpus.
    NaijaNER,
    /// Foundation model training collection for bioacoustics. Multi
    NatureLMAudio,
    /// Robustness benchmark for NER. 6 real noise types: expert, cr
    NoiseBench,
    /// Norwegian NER covering Bokmål and Nynorsk. Morphologically r
    NorNE,
    /// Open Richly Annotated Cuneiform Corpus. Sumerian, Akkadian, 
    ORACC,
    /// Diverse fine-grained Chinese NER covering informal text (soc
    OmniNER2025,
    /// Penn Discourse TreeBank v3. 43 discourse relation types.
    PDTBv3,
    /// 200k synthetic examples for PII detection and masking. Cover
    PIIMasking200k,
    /// PubMed abstracts with discontinuous biomedical entities. Com
    PubMedDiscontinuous,
    /// French historical newspaper NER from 1890. OCR-corrected wit
    QuaeroOldPress,
    /// 100k prompts for measuring toxicity generation in language m
    RealToxicityPrompts,
    /// Zero-shot NER evaluation suite across 20 diverse datasets. T
    ReasoningNER,
    /// Deep learning recipe NER. Multi-scale datasets with ingredie
    RecipeNER,
    /// Robustness benchmark for NER. Real-world adversarial example
    RockNER,
    /// Species-800 corpus. Species name recognition in biomedical t
    S800,
    /// Scientific paper NER with nested annotations. Methods, tasks
    SCINERNested,
    /// Clinical entity linking to SNOMED CT. From SNOMED Internatio
    SNOMEDChallenge,
    /// Scientific cross-document concept coreference. Dynamic defin
    SciCoRadar,
    /// Scientific information extraction from AI/ML papers. Nested 
    SciERC,
    /// Long-document QA from SCROLLS benchmark. Query-focused meeti
    ScrollsQMSum,
    /// Clinical disorder mentions from ShARe/CLEF eHealth 2013. Dis
    ShARe2013,
    /// Clinical disorder mentions from ShARe/CLEF eHealth 2014. Imp
    ShARe2014,
    /// Shared Annotated Resources for clinical NER. ShARe/CLEF eHea
    ShAReCLEF,
    /// Anaphoric shell noun resolution. 670 English shell nouns fro
    ShellNouns,
    /// Measuring stereotypical bias in language models. 4 target do
    StereoSet,
    /// Streaming cross-document entity coreference protocol. News d
    StreamingCDCoref,
    /// Recipe ingredient NER. 700 annotated recipe ingredient lists
    TASTEset,
    /// Temporal Histories of Your Medical Events. Clinical temporal
    THYME,
    /// POS-tagged Esperanto from Parallel Bible Corpus. ~1800 sente
    TaggedPBCEsperanto,
    /// POS-tagged Klingon from Parallel Bible Corpus. OVS word orde
    TaggedPBCKlingon,
    /// Temporal document-level relation extraction. Converts static
    TemDocRED,
    /// Temporal annotation benchmark. TIMEX, EVENT spans, and tempo
    TempEval3,
    /// Canonical temporal IE corpus. News articles with TIMEX3, eve
    TimeBank12,
    /// Dense temporal relation annotation. Re-annotation of TimeBan
    TimeBankDense,
    /// Toki Pona minimalist language corpus. 120-word language for 
    TokiPonaCorpus,
    /// Twitter NER + Entity Linking. End-to-end NERD benchmark span
    TweetNERD,
    /// Multimodal NER on Twitter. Text + image for entity recogniti
    Twitter2015MNER,
    /// Grounded Multimodal NER. Entities linked to bounding boxes i
    TwitterGMNER,
    /// Multilingual Multimodal NER. Four languages with text-image 
    TwoMNER,
    /// Universal Dependencies treebank for Esperanto. Syntax annota
    UDEsperantoCairo,
    /// Astrophysics NER from NASA ADS. 31 entity types: facilities,
    WIESP2022NER,
    /// Web-scale entity linking from ClueWeb corpus. Tests EL on no
    WNEDClueweb,
    /// Large-scale Wikipedia entity linking dataset extracted from 
    WNEDWiki,
    /// Twitter NER workshop shared task. Focus on rare and emerging
    WNUT16,
    /// Named entity recognition for Welsh (Cymraeg). Celtic languag
    WelshNER,
    /// Semantic drift detection in Wikidata. LLM-based classificati
    WikidataDrift,
    /// Entity disambiguation benchmark. 95k Wikipedia paragraphs, 8
    ZELDA,
    /// Joint coreference and zero-pronoun resolution. For languages
    Zcoref,

// === DOWNLOAD URL MATCH ARMS ===
// Add these to `fn download_url(&self)`

            DatasetId::ACE05RE => ""  // TODO: Add download URL,
            DatasetId::ADRDiscontinuous => "https://github.com/Aitslab/ADR-DisNER",
            DatasetId::AIDACoNLL => "https://www.mpi-inf.mpg.de/departments/databases-and-information-systems/research/ambiverse-nlu/aida",
            DatasetId::AQUAINT => ""  // TODO: Add download URL,
            DatasetId::AgCNER => "https://github.com/AgCNER/AgCNER",
            DatasetId::AnaphoraAccessibility => ""  // TODO: Add download URL,
            DatasetId::AnnoCTR => "https://github.com/boschresearch/anno-ctr-lrec-coling-2024/archive/refs/heads/main.zip",
            DatasetId::BEANSZero => "https://github.com/earthspecies/beans-zero",
            DatasetId::BELB => "https://github.com/sg-wbi/belb",
            DatasetId::BasqueNER => "https://github.com/ixa-ehu/eusner",
            DatasetId::BioNERLLaMA => "https://github.com/BioNER-LLaMA/BioNER-LLaMA",
            DatasetId::BoldBias => "https://github.com/amazon-science/bold",
            DatasetId::BookCoref => "https://huggingface.co/datasets/spacemanidol/BookCoref",
            DatasetId::BookSumCoref => "https://github.com/salesforce/booksum",
            DatasetId::CALCS2018 => "https://code-switching.github.io/2018/",
            DatasetId::CBMACharters => ""  // TODO: Add download URL,
            DatasetId::CHEMDNER => "https://biocreative.bioinformatics.udel.edu/tasks/biocreative-iv/chemdner/",
            DatasetId::CODICRACBridging => "https://github.com/UniversalAnaphora/UA-CODI-CRAC",
            DatasetId::ChineseNestedNER => "https://github.com/LeeSureman/Nested-NER",
            DatasetId::CoNLL04RE => "https://github.com/bekou/multihead_joint_entity_relation_extraction",
            DatasetId::CoQAEntities => "https://stanfordnlp.github.io/coqa/",
            DatasetId::CodeSearchNet => "https://github.com/github/CodeSearchNet",
            DatasetId::CopticScriptorium => "https://data.copticscriptorium.org/",
            DatasetId::CrossRE => "https://github.com/mainlp/CrossRE",
            DatasetId::CrossWeigh => "https://github.com/ZihanWangKi/CrossWeigh",
            DatasetId::CrowSPairs => "https://github.com/nyu-mll/crows-pairs",
            DatasetId::DialogRE => "https://github.com/nlpdata/dialogre",
            DatasetId::DistantListeningCorpus => "https://zenodo.org/records/15150283",
            DatasetId::DutchArchaeology => "https://live.european-language-grid.eu/catalogue/corpus/13410",
            DatasetId::ELGold => "https://mostwiedzy.pl/en/open-research-data/elgold-gold-standard-multi-genre-dataset",
            DatasetId::ENERSec => "https://github.com/jnishii/E-NER",
            DatasetId::ENer => "https://raw.githubusercontent.com/terenceau1/E-NER-Dataset/main/all.csv",
            DatasetId::ESCOSkillsEL => ""  // TODO: Add download URL,
            DatasetId::EnzChemRED => "https://github.com/EnzChemRED/EnzChemRED",
            DatasetId::EventKGDrift => "https://research.tue.nl/files/349781334/978-3-031-61057-8_9.pdf",
            DatasetId::FABLE => "https://huggingface.co/DeBERTa-literary-entities",
            DatasetId::FCC => ""  // TODO: Add download URL,
            DatasetId::FINER => "https://figshare.com/ndownloader/files/36144501",
            DatasetId::FiNER139 => "https://github.com/FiNER-139/FiNER-139",
            DatasetId::FinBenNER => "https://github.com/TheFinAI/FinBen",
            DatasetId::GeoWebNews => "https://github.com/milangritta/GeoWebNews",
            DatasetId::GermEvalDiscontinuous => "https://sites.google.com/site/germaboreval/data",
            DatasetId::HinglishNER => "https://github.com/murali1996/CodemixedNLP",
            DatasetId::HistNERo => "https://github.com/UniBuc-HistNERo/HistNERo",
            DatasetId::I2B22010 => ""  // TODO: Add download URL,
            DatasetId::I2B2Temporal => ""  // TODO: Add download URL,
            DatasetId::IndicNER => "https://github.com/AI4Bharat/IndicNER",
            DatasetId::InterlingueWikipedia => "https://dumps.wikimedia.org/iewiki/",
            DatasetId::KORE50 => "https://github.com/KORE50/KORE50-NIF-NER",
            DatasetId::KlingonEffectLID => "https://wmdqs.org/submissions-2025/19.pdf",
            DatasetId::LEMONADE => "https://github.com/lemonade-coref/lemonade",
            DatasetId::LGL => "https://github.com/wikipedia2vec/wikipedia2vec",
            DatasetId::LT4HALA => ""  // TODO: Add download URL,
            DatasetId::LatinUD => "https://raw.githubusercontent.com/UniversalDependencies/UD_Latin-ITTB/master/la_ittb-ud-test.conllu",
            DatasetId::LegalCore => ""  // TODO: Add download URL,
            DatasetId::LexGLUENER => "https://github.com/coastalcph/lex-glue",
            DatasetId::LojbanTatoeba => "https://tatoeba.org/en/downloads",
            DatasetId::LongDocNER => "https://github.com/xhuang28/LongDocNER",
            DatasetId::MACCROBAT => "https://figshare.com/articles/dataset/MACCROBAT2018/9764942",
            DatasetId::MATRES => "https://github.com/qiangning/MATRES",
            DatasetId::MEANTIME => "https://github.com/newsreader/meantime",
            DatasetId::MELO => "https://github.com/avature/melo-benchmark",
            DatasetId::MHERCL => "https://arxiv.org/html/2505.03473v1",
            DatasetId::MNERMI => "https://github.com/NUSTM/MNER-MI",
            DatasetId::MSNBCEL => ""  // TODO: Add download URL,
            DatasetId::MaoriNER => ""  // TODO: Add download URL,
            DatasetId::MathEntities => "https://github.com/dmazzei/mathematical-entities",
            DatasetId::MedMentions => "https://github.com/chanzuckerberg/MedMentions",
            DatasetId::MedievalCharterNER => "https://zenodo.org/records/6463699",
            DatasetId::MentionResolutionLLM => "https://github.com/mention-resolution/mention-resolution-llm",
            DatasetId::MultiBioNERLong => "https://github.com/dmis-lab/multi-bio-ner",
            DatasetId::MultiWOZNER => "https://github.com/budzianowski/multiwoz",
            DatasetId::NCERB => "https://github.com/NCERB/NCERB",
            DatasetId::NYT10 => "http://iesl.cs.umass.edu/riedel/ecml/",
            DatasetId::NaijaNER => ""  // TODO: Add download URL,
            DatasetId::NatureLMAudio => "https://github.com/earthspecies/naturelm-audio",
            DatasetId::NoiseBench => "https://github.com/elenamer/NoiseBench",
            DatasetId::NorNE => "https://github.com/ltgoslo/norne",
            DatasetId::ORACC => "http://oracc.museum.upenn.edu/",
            DatasetId::OmniNER2025 => ""  // TODO: Add download URL,
            DatasetId::PDTBv3 => ""  // TODO: Add download URL,
            DatasetId::PIIMasking200k => "https://huggingface.co/datasets/ai4privacy/pii-masking-200k",
            DatasetId::PubMedDiscontinuous => "https://github.com/dmis-lab/discontinuous-ner",
            DatasetId::QuaeroOldPress => ""  // TODO: Add download URL,
            DatasetId::RealToxicityPrompts => "https://huggingface.co/datasets/allenai/real-toxicity-prompts",
            DatasetId::ReasoningNER => "https://github.com/reasoning-ner/reasoning-ner",
            DatasetId::RecipeNER => "https://github.com/cosylabiiit/recipe-ner",
            DatasetId::RockNER => "https://github.com/INK-USC/RockNER",
            DatasetId::S800 => "https://species.jensenlab.org/files/S800-1.0.tar.gz",
            DatasetId::SCINERNested => "https://github.com/allenai/sciie",
            DatasetId::SNOMEDChallenge => ""  // TODO: Add download URL,
            DatasetId::SciCoRadar => "https://github.com/allenai/scico-radar",
            DatasetId::SciERC => "https://nlp.cs.washington.edu/sciIE/",
            DatasetId::ScrollsQMSum => "https://github.com/tau-nlp/scrolls",
            DatasetId::ShARe2013 => ""  // TODO: Add download URL,
            DatasetId::ShARe2014 => ""  // TODO: Add download URL,
            DatasetId::ShAReCLEF => ""  // TODO: Add download URL,
            DatasetId::ShellNouns => ""  // TODO: Add download URL,
            DatasetId::StereoSet => "https://github.com/moinnadeem/StereoSet",
            DatasetId::StreamingCDCoref => "https://www.cs.jhu.edu/~mdredze/publications/streaming_coref_coling.pdf",
            DatasetId::TASTEset => "https://github.com/taisti/TASTEset",
            DatasetId::THYME => ""  // TODO: Add download URL,
            DatasetId::TaggedPBCEsperanto => "https://github.com/clab/taggedPBC",
            DatasetId::TaggedPBCKlingon => "https://github.com/clab/taggedPBC",
            DatasetId::TemDocRED => "https://github.com/THUDM/Tem-DocRED",
            DatasetId::TempEval3 => "https://figshare.com/articles/dataset/TempEval-3_data/9586532",
            DatasetId::TimeBank12 => "https://catalog.ldc.upenn.edu/LDC2006T08",
            DatasetId::TimeBankDense => "https://github.com/bethard/timebank-dense",
            DatasetId::TokiPonaCorpus => "https://github.com/kilipan/toki-pona-corpus",
            DatasetId::TweetNERD => "https://zenodo.org/records/6617192",
            DatasetId::Twitter2015MNER => "https://github.com/jefferyYu/UMT",
            DatasetId::TwitterGMNER => "https://github.com/JinYuanLi0012/RiVEG",
            DatasetId::TwoMNER => "https://github.com/Alibaba-NLP/2M-NER",
            DatasetId::UDEsperantoCairo => "https://raw.githubusercontent.com/UniversalDependencies/UD_Esperanto-Cairo/master/eo_cairo-ud-test.conllu",
            DatasetId::WIESP2022NER => "https://huggingface.co/datasets/adsabs/WIESP2022-NER",
            DatasetId::WNEDClueweb => ""  // TODO: Add download URL,
            DatasetId::WNEDWiki => "https://github.com/wikipedia2vec/wikipedia2vec",
            DatasetId::WNUT16 => "https://raw.githubusercontent.com/napsternxg/TwitterNER/master/data/wnut16/test",
            DatasetId::WelshNER => "https://github.com/Portulan/welsh-ner",
            DatasetId::WikidataDrift => "https://arxiv.org/abs/2511.04926",
            DatasetId::ZELDA => "https://github.com/flairNLP/zelda",
            DatasetId::Zcoref => ""  // TODO: Add download URL,

// === NAME MATCH ARMS ===
// Add these to `fn name(&self)`

            DatasetId::ACE05RE => "ACE 2005 RE",
            DatasetId::ADRDiscontinuous => "ADR Discontinuous",
            DatasetId::AIDACoNLL => "AIDA-CoNLL",
            DatasetId::AQUAINT => "AQUAINT",
            DatasetId::AgCNER => "AgCNER",
            DatasetId::AnaphoraAccessibility => "Anaphora Accessibility",
            DatasetId::AnnoCTR => "AnnoCTR (Cyber Threat Reports)",
            DatasetId::BEANSZero => "BEANS-Zero",
            DatasetId::BELB => "BELB",
            DatasetId::BasqueNER => "Basque NER",
            DatasetId::BioNERLLaMA => "BioNER-LLaMA",
            DatasetId::BoldBias => "BOLD",
            DatasetId::BookCoref => "BookCoref",
            DatasetId::BookSumCoref => "BookSum Coref",
            DatasetId::CALCS2018 => "CALCS-2018",
            DatasetId::CBMACharters => "CBMA Charters",
            DatasetId::CHEMDNER => "CHEMDNER",
            DatasetId::CODICRACBridging => "CODI-CRAC Bridging",
            DatasetId::ChineseNestedNER => "Chinese Nested NER",
            DatasetId::CoNLL04RE => "CoNLL04 RE",
            DatasetId::CoQAEntities => "CoQA",
            DatasetId::CodeSearchNet => "CodeSearchNet",
            DatasetId::CopticScriptorium => "Coptic Scriptorium",
            DatasetId::CrossRE => "CrossRE",
            DatasetId::CrossWeigh => "CrossWeigh",
            DatasetId::CrowSPairs => "CrowS-Pairs",
            DatasetId::DialogRE => "DialogRE",
            DatasetId::DistantListeningCorpus => "Distant Listening Corpus",
            DatasetId::DutchArchaeology => "Dutch Archaeology NER",
            DatasetId::ELGold => "ELGold",
            DatasetId::ENERSec => "E-NER SEC",
            DatasetId::ENer => "E-NER (EDGAR-NER)",
            DatasetId::ESCOSkillsEL => "ESCO Skills EL",
            DatasetId::EnzChemRED => "EnzChemRED",
            DatasetId::EventKGDrift => "Event KG Drift",
            DatasetId::FABLE => "FABLE",
            DatasetId::FCC => "Football Coreference Corpus",
            DatasetId::FINER => "FINER (Food Ingredients NER)",
            DatasetId::FiNER139 => "FiNER-139",
            DatasetId::FinBenNER => "FinBen NER",
            DatasetId::GeoWebNews => "GeoWebNews",
            DatasetId::GermEvalDiscontinuous => "GermEval Discontinuous",
            DatasetId::HinglishNER => "Hinglish NER",
            DatasetId::HistNERo => "HistNERo",
            DatasetId::I2B22010 => "i2b2 2010",
            DatasetId::I2B2Temporal => "i2b2 2012 Temporal",
            DatasetId::IndicNER => "IndicNER",
            DatasetId::InterlingueWikipedia => "Interlingue Wikipedia",
            DatasetId::KORE50 => "KORE50",
            DatasetId::KlingonEffectLID => "Klingon Effect LID",
            DatasetId::LEMONADE => "LEMONADE",
            DatasetId::LGL => "LGL",
            DatasetId::LT4HALA => "LT4HALA Hebrew",
            DatasetId::LatinUD => "Latin UD",
            DatasetId::LegalCore => "LegalCore",
            DatasetId::LexGLUENER => "LexGLUE NER",
            DatasetId::LojbanTatoeba => "Lojban Tatoeba",
            DatasetId::LongDocNER => "Long Document NER",
            DatasetId::MACCROBAT => "MACCROBAT",
            DatasetId::MATRES => "MATRES",
            DatasetId::MEANTIME => "MEANTIME",
            DatasetId::MELO => "MELO",
            DatasetId::MHERCL => "MHERCL",
            DatasetId::MNERMI => "MNER-MI",
            DatasetId::MSNBCEL => "MSNBC",
            DatasetId::MaoriNER => "Māori NER",
            DatasetId::MathEntities => "Mathematical Entities",
            DatasetId::MedMentions => "MedMentions",
            DatasetId::MedievalCharterNER => "Medieval Charter NER",
            DatasetId::MentionResolutionLLM => "Mention Resolution LLM",
            DatasetId::MultiBioNERLong => "Multi-Bio Long NER",
            DatasetId::MultiWOZNER => "MultiWOZ NER",
            DatasetId::NCERB => "NCERB",
            DatasetId::NYT10 => "NYT-10",
            DatasetId::NaijaNER => "NaijaNER",
            DatasetId::NatureLMAudio => "NatureLM-audio",
            DatasetId::NoiseBench => "NoiseBench",
            DatasetId::NorNE => "NorNE",
            DatasetId::ORACC => "ORACC",
            DatasetId::OmniNER2025 => "OmniNER2025",
            DatasetId::PDTBv3 => "PDTB 3.0",
            DatasetId::PIIMasking200k => "PII Masking 200k",
            DatasetId::PubMedDiscontinuous => "PubMed Discontinuous",
            DatasetId::QuaeroOldPress => "Quaero Old Press",
            DatasetId::RealToxicityPrompts => "RealToxicityPrompts",
            DatasetId::ReasoningNER => "ReasoningNER",
            DatasetId::RecipeNER => "Recipe NER",
            DatasetId::RockNER => "RockNER",
            DatasetId::S800 => "S800",
            DatasetId::SCINERNested => "SciNER Nested",
            DatasetId::SNOMEDChallenge => "SNOMED CT EL Challenge",
            DatasetId::SciCoRadar => "SciCo-Radar",
            DatasetId::SciERC => "SciERC",
            DatasetId::ScrollsQMSum => "SCROLLS QMSum",
            DatasetId::ShARe2013 => "ShARe 2013",
            DatasetId::ShARe2014 => "ShARe 2014",
            DatasetId::ShAReCLEF => "ShARe/CLEF",
            DatasetId::ShellNouns => "Shell Nouns (ASN)",
            DatasetId::StereoSet => "StereoSet",
            DatasetId::StreamingCDCoref => "Streaming CD-Coref",
            DatasetId::TASTEset => "TASTEset",
            DatasetId::THYME => "THYME",
            DatasetId::TaggedPBCEsperanto => "taggedPBC Esperanto",
            DatasetId::TaggedPBCKlingon => "taggedPBC Klingon",
            DatasetId::TemDocRED => "Tem-DocRED",
            DatasetId::TempEval3 => "TempEval-3",
            DatasetId::TimeBank12 => "TimeBank 1.2",
            DatasetId::TimeBankDense => "TimeBank-Dense",
            DatasetId::TokiPonaCorpus => "Toki Pona Corpus",
            DatasetId::TweetNERD => "TweetNERD",
            DatasetId::Twitter2015MNER => "Twitter-2015 MNER",
            DatasetId::TwitterGMNER => "Twitter-GMNER",
            DatasetId::TwoMNER => "2M-NER",
            DatasetId::UDEsperantoCairo => "UD Esperanto Cairo",
            DatasetId::WIESP2022NER => "WIESP2022-NER (DEAL)",
            DatasetId::WNEDClueweb => "WNED-ClueWeb",
            DatasetId::WNEDWiki => "WNED-WIKI",
            DatasetId::WNUT16 => "WNUT-16",
            DatasetId::WelshNER => "Welsh NER",
            DatasetId::WikidataDrift => "Wikidata Semantic Drift",
            DatasetId::ZELDA => "ZELDA",
            DatasetId::Zcoref => "Z-coref",

// === DESCRIPTION MATCH ARMS ===
// Add these to `fn description(&self)`

            DatasetId::ACE05RE => "ACE 2005 relation extraction component. 7 entity types, 6 relation types with subtypes.",
            DatasetId::ADRDiscontinuous => "Adverse Drug Reaction corpus with discontinuous mentions. Patient forum posts.",
            DatasetId::AIDACoNLL => "Primary entity linking benchmark linking CoNLL-2003 mentions to Wikipedia. De-facto standard for ...",
            DatasetId::AQUAINT => "Newswire entity linking dataset from AQUAINT corpus. Wikipedia-linked mentions.",
            DatasetId::AgCNER => "Large-scale Chinese agricultural NER. 66k samples, ~207k entities, 3.9M characters.",
            DatasetId::AnaphoraAccessibility => "Discourse anaphora accessibility evaluation. Tests non-NP antecedents.",
            DatasetId::AnnoCTR => "Cyber threat intelligence NER with MITRE ATT&CK linking. 400 annotated documents from commercial ...",
            DatasetId::BEANSZero => "Bioacoustics benchmark beyond species classification. Natural-language prompts for animal sounds.",
            DatasetId::BELB => "Biomedical Entity Linking Benchmark unifying 11 corpora across 7 knowledge bases. Standardized bi...",
            DatasetId::BasqueNER => "Named entity recognition for Basque (Euskara). Language isolate NER corpus.",
            DatasetId::BioNERLLaMA => "Instruction-tuned biomedical NER benchmark. Evaluates generative models on disease/chemical/gene ...",
            DatasetId::BoldBias => "Bias in Open-ended Language Generation Dataset. Wikipedia-based prompts.",
            DatasetId::BookCoref => "Full-novel coreference with automatic silver and manual gold annotations. Includes Animal Farm, S...",
            DatasetId::BookSumCoref => "Coreference annotations on book chapters from BookSum. Long literary texts.",
            DatasetId::CALCS2018 => "Code-Switching Workshop shared task. English-Spanish Twitter NER with 9 entity types.",
            DatasetId::CBMACharters => "Burgundian medieval Latin charters NER. 9th-14th century diplomatic documents.",
            DatasetId::CHEMDNER => "Chemical compound and drug name recognition in scientific text.",
            DatasetId::CODICRACBridging => "Universal Anaphora bridging annotations. One of the largest bridging datasets.",
            DatasetId::ChineseNestedNER => "Chinese nested named entity recognition. Multiple levels of embedded entities.",
            DatasetId::CoNLL04RE => "Sentence-level relation extraction from CoNLL-2004. Clean, small RE benchmark.",
            DatasetId::CoQAEntities => "Conversational Question Answering. Multi-turn QA requiring entity mention resolution.",
            DatasetId::CodeSearchNet => "Code understanding benchmark. Function documentation and code search across 6 languages.",
            DatasetId::CopticScriptorium => "Sahidic Coptic with multi-layer annotation. ~50k tokens.",
            DatasetId::CrossRE => "Cross-domain relation extraction across 6 domains. Tests RE generalization.",
            DatasetId::CrossWeigh => "Cross-lingual adversarial NER evaluation. Tests multilingual model robustness.",
            DatasetId::CrowSPairs => "Crowdsourced stereotype pairs benchmark. 9 bias categories for language models.",
            DatasetId::DialogRE => "Dialogue-based relation extraction. Multi-turn conversations requiring entity tracking across turns.",
            DatasetId::DistantListeningCorpus => "1,283 musical scores with harmonic annotations. String quartet + piano music with Roman numeral a...",
            DatasetId::DutchArchaeology => "Archaeological excavation reports from DANS archive. 31k annotations across 6 entity types.",
            DatasetId::ELGold => "Gold-standard multi-genre Polish NER+EL. Includes fiction, press, blogs.",
            DatasetId::ENERSec => "Legal NER from SEC EDGAR filings. 52 documents with financial entity annotations.",
            DatasetId::ENer => "NER for US SEC EDGAR filings. 52 documents, 400k+ tokens with legal entities.",
            DatasetId::ESCOSkillsEL => "Entity linking for occupational skills to ESCO taxonomy. Job market domain, multilingual.",
            DatasetId::EnzChemRED => "Enzyme chemistry relation extraction. Links enzymes, substrates, products, cofactors from biochem...",
            DatasetId::EventKGDrift => "Multi-perspective concept drift detection on event knowledge graphs.",
            DatasetId::FABLE => "Fiction Adapted BERT for Literary Entities. DeBERTa-based NER for narrative fiction.",
            DatasetId::FCC => "Cross-document event coreference for football matches. Requires temporal reasoning.",
            DatasetId::FINER => "Food ingredient NER from AllRecipes. 182k sentences with ingredient phrases in IOB2 format.",
            DatasetId::FiNER139 => "Financial NER with 139 fine-grained entity types. SEC 10-K/10-Q filings.",
            DatasetId::FinBenNER => "Financial NER from FinBen benchmark. Entity extraction from financial documents and filings.",
            DatasetId::GeoWebNews => "Geoparsing benchmark from web news. Toponyms with geocoding coordinates.",
            DatasetId::GermEvalDiscontinuous => "German discontinuous NER from GermEval 2014. Non-contiguous entity spans.",
            DatasetId::HinglishNER => "Hindi-English code-mixed social media NER. Roman script Hindi mixed with English.",
            DatasetId::HistNERo => "Romanian historical newspaper NER. First Romanian historical NER corpus from four regions.",
            DatasetId::I2B22010 => "Clinical concept extraction and assertion classification. Foundational clinical NER benchmark.",
            DatasetId::I2B2Temporal => "Clinical temporal relations challenge. Events, TIMEX3, and TLINKs in discharge summaries.",
            DatasetId::IndicNER => "Indian languages NER covering 11 Indian languages. Low-resource multilingual NER.",
            DatasetId::InterlingueWikipedia => "Interlingue (Occidental) Wikipedia text corpus. International auxiliary language.",
            DatasetId::KORE50 => "Short, highly ambiguous entity linking snippets. Tests disambiguation difficulty.",
            DatasetId::KlingonEffectLID => "Language ID dataset with 11 constructed languages. 14.2M sentences across 101 languages.",
            DatasetId::LEMONADE => "Large-scale multilingual conflict event corpus. 39k events across 20 languages for CDEC search.",
            DatasetId::LGL => "Local-Global Lexicon for toponym disambiguation. News articles with geolocation.",
            DatasetId::LT4HALA => "Biblical Hebrew NER and coreference annotation.",
            DatasetId::LatinUD => "Universal Dependencies for Latin. Classical through Medieval.",
            DatasetId::LegalCore => "Event coreference in long legal documents. Long-distance cross-section event links.",
            DatasetId::LexGLUENER => "Legal NER from LexGLUE benchmark. Legal entity extraction from case law and contracts.",
            DatasetId::LojbanTatoeba => "Lojban-English sentence pairs from Tatoeba. Logical language translation corpus.",
            DatasetId::LongDocNER => "Long-document NER benchmark. Tests entity recognition across extended contexts.",
            DatasetId::MACCROBAT => "Biomedical NER corpus with extensive coverage. Used with RoBERTa-WWM and deep models.",
            DatasetId::MATRES => "Multi-Axis Temporal Relations. Cleaner, more consistent event-event temporal relation annotations.",
            DatasetId::MEANTIME => "Multilingual news corpus with within- and cross-document event coreference. 4 languages.",
            DatasetId::MELO => "Multilingual Entity Linking of Occupations. 48 datasets across 21 languages for occupation EL.",
            DatasetId::MHERCL => "Historical long-tail entity linking benchmark. Tests LLM behavior on rare/historical Wikidata ent...",
            DatasetId::MNERMI => "Multimodal NER with Multiple Images. Social media posts with multiple image context.",
            DatasetId::MSNBCEL => "Small news article entity linking dataset. Commonly used for out-of-domain EL evaluation.",
            DatasetId::MaoriNER => "Named entity recognition for Te Reo Māori. New Zealand indigenous language corpus.",
            DatasetId::MathEntities => "Terminology and definition extraction from mathematical text. Category theory corpora.",
            DatasetId::MedMentions => "Large-scale biomedical concept mentions mapped to UMLS. PubMed abstracts with fine-grained semant...",
            DatasetId::MedievalCharterNER => "Multilingual medieval charter NER. Latin, French, Spanish from major charter collections.",
            DatasetId::MentionResolutionLLM => "MCQ-format coreference for LLMs from LitBank and FantasyCoref. Tests referential understanding on...",
            DatasetId::MultiBioNERLong => "Long biomedical document NER. Full-text articles vs abstracts.",
            DatasetId::MultiWOZNER => "Multi-domain task-oriented dialogue with slot/entity annotations. Multi-turn conversations.",
            DatasetId::NCERB => "Named Clinical Entity Recognition Benchmark. Multi-dataset clinical NER evaluation suite.",
            DatasetId::NYT10 => "New York Times distant supervision RE. 24 Freebase relations.",
            DatasetId::NaijaNER => "Nigerian Pidgin NER corpus.",
            DatasetId::NatureLMAudio => "Foundation model training collection for bioacoustics. Multi-species audio-text pairs.",
            DatasetId::NoiseBench => "Robustness benchmark for NER. 6 real noise types: expert, crowd, LLM, distant/weak supervision.",
            DatasetId::NorNE => "Norwegian NER covering Bokmål and Nynorsk. Morphologically rich language NER.",
            DatasetId::ORACC => "Open Richly Annotated Cuneiform Corpus. Sumerian, Akkadian, Urartian.",
            DatasetId::OmniNER2025 => "Diverse fine-grained Chinese NER covering informal text (social media, forums). Large-scale bench...",
            DatasetId::PDTBv3 => "Penn Discourse TreeBank v3. 43 discourse relation types.",
            DatasetId::PIIMasking200k => "200k synthetic examples for PII detection and masking. Covers 50+ PII types.",
            DatasetId::PubMedDiscontinuous => "PubMed abstracts with discontinuous biomedical entities. Complex entity boundaries.",
            DatasetId::QuaeroOldPress => "French historical newspaper NER from 1890. OCR-corrected with manual NE annotations.",
            DatasetId::RealToxicityPrompts => "100k prompts for measuring toxicity generation in language models.",
            DatasetId::ReasoningNER => "Zero-shot NER evaluation suite across 20 diverse datasets. Tests LLM NER capabilities.",
            DatasetId::RecipeNER => "Deep learning recipe NER. Multi-scale datasets with ingredient and instruction entities.",
            DatasetId::RockNER => "Robustness benchmark for NER. Real-world adversarial examples with boundary ambiguity.",
            DatasetId::S800 => "Species-800 corpus. Species name recognition in biomedical text.",
            DatasetId::SCINERNested => "Scientific paper NER with nested annotations. Methods, tasks, and datasets.",
            DatasetId::SNOMEDChallenge => "Clinical entity linking to SNOMED CT. From SNOMED International 2024 challenge.",
            DatasetId::SciCoRadar => "Scientific cross-document concept coreference. Dynamic definitions via LLM retrieval.",
            DatasetId::SciERC => "Scientific information extraction from AI/ML papers. Nested entities and relations.",
            DatasetId::ScrollsQMSum => "Long-document QA from SCROLLS benchmark. Query-focused meeting summarization.",
            DatasetId::ShARe2013 => "Clinical disorder mentions from ShARe/CLEF eHealth 2013. Discontinuous entity annotations.",
            DatasetId::ShARe2014 => "Clinical disorder mentions from ShARe/CLEF eHealth 2014. Improved discontinuous NER annotations.",
            DatasetId::ShAReCLEF => "Shared Annotated Resources for clinical NER. ShARe/CLEF eHealth shared task.",
            DatasetId::ShellNouns => "Anaphoric shell noun resolution. 670 English shell nouns from Schmid taxonomy.",
            DatasetId::StereoSet => "Measuring stereotypical bias in language models. 4 target domains.",
            DatasetId::StreamingCDCoref => "Streaming cross-document entity coreference protocol. News domain streaming evaluation.",
            DatasetId::TASTEset => "Recipe ingredient NER. 700 annotated recipe ingredient lists with 9 entity classes.",
            DatasetId::THYME => "Temporal Histories of Your Medical Events. Clinical temporal IE with events and relations.",
            DatasetId::TaggedPBCEsperanto => "POS-tagged Esperanto from Parallel Bible Corpus. ~1800 sentences with word-level alignment.",
            DatasetId::TaggedPBCKlingon => "POS-tagged Klingon from Parallel Bible Corpus. OVS word order with complex verbal morphology.",
            DatasetId::TemDocRED => "Temporal document-level relation extraction. Converts static triples to temporal quadruples.",
            DatasetId::TempEval3 => "Temporal annotation benchmark. TIMEX, EVENT spans, and temporal relations.",
            DatasetId::TimeBank12 => "Canonical temporal IE corpus. News articles with TIMEX3, events, and temporal links (TLINKs).",
            DatasetId::TimeBankDense => "Dense temporal relation annotation. Re-annotation of TimeBank with more consistent TLINK labels.",
            DatasetId::TokiPonaCorpus => "Toki Pona minimalist language corpus. 120-word language for semantic simplification.",
            DatasetId::TweetNERD => "Twitter NER + Entity Linking. End-to-end NERD benchmark spanning 2010-2021.",
            DatasetId::Twitter2015MNER => "Multimodal NER on Twitter. Text + image for entity recognition.",
            DatasetId::TwitterGMNER => "Grounded Multimodal NER. Entities linked to bounding boxes in social media images.",
            DatasetId::TwoMNER => "Multilingual Multimodal NER. Four languages with text-image pairs.",
            DatasetId::UDEsperantoCairo => "Universal Dependencies treebank for Esperanto. Syntax annotation without NER layer.",
            DatasetId::WIESP2022NER => "Astrophysics NER from NASA ADS. 31 entity types: facilities, wavelengths, telescopes, archives.",
            DatasetId::WNEDClueweb => "Web-scale entity linking from ClueWeb corpus. Tests EL on noisy web text.",
            DatasetId::WNEDWiki => "Large-scale Wikipedia entity linking dataset extracted from Wikipedia hyperlinks.",
            DatasetId::WNUT16 => "Twitter NER workshop shared task. Focus on rare and emerging entities in noisy social media text.",
            DatasetId::WelshNER => "Named entity recognition for Welsh (Cymraeg). Celtic language NER corpus.",
            DatasetId::WikidataDrift => "Semantic drift detection in Wikidata. LLM-based classification inconsistency detection.",
            DatasetId::ZELDA => "Entity disambiguation benchmark. 95k Wikipedia paragraphs, 8 ED datasets unified.",
            DatasetId::Zcoref => "Joint coreference and zero-pronoun resolution. For languages with pro-drop (Chinese, Japanese, Ko...",

