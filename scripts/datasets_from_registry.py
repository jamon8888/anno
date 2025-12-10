# Auto-generated from dataset_registry.rs - DO NOT EDIT MANUALLY
# Run: cargo test generate_python_download_config -- --ignored
# Then copy this to scripts/download_extended_datasets.py

DATASETS_FROM_REGISTRY = {
    "wnut17": {
        "group": "social_media",
        "hf_dataset": "leondz/wnut_17",
        "output": "wnut17.json",
        "description": "Social media NER with emerging entities. Created to evaluate models on rare/emerging entities in noisy social text.",
    },
    "multi_nerd": {
        "group": "wikipedia",
        "hf_dataset": "Babelscape/multinerd",
        "output": "multi_nerd.json",
        "description": "Large multilingual NER covering 10 languages. Created to address scarcity of multilingual fine-grained NER data.",
    },
    "few_nerd": {
        "group": "wikipedia",
        "hf_dataset": "DFKI-SLT/few-nerd",
        "output": "few_nerd.json",
        "description": "Fine-grained NER with 66 types in 8 coarse categories. Designed for few-shot learning evaluation.",
    },
    "cross_ner": {
        "group": "multi-domain",
        "hf_dataset": "DFKI-SLT/cross_ner",
        "config": "politics",
        "output": "cross_ner.json",
        "description": "Cross-domain NER across 5 domains: politics, science, music, literature, AI. Tests domain transfer.",
    },
    "fab_ner": {
        "group": "manufacturing",
        "hf_dataset": "DFKI-SLT/fabner",
        "output": "fab_ner.json",
        "description": "Manufacturing domain NER. 12 entity types for Industry 4.0 applications.",
    },
    "broad_twitter_corpus": {
        "group": "social_media",
        "hf_dataset": "tner/btc",
        "output": "broad_twitter_corpus.json",
        "description": "Twitter NER across multiple time periods. Tests temporal robustness of NER systems.",
    },
    "wiki_neural": {
        "group": "wikipedia",
        "hf_dataset": "Babelscape/wikineural",
        "config": "en",
        "output": "wiki_neural.json",
        "description": "Silver-standard multilingual NER from Wikipedia. 9 languages with automatic annotation.",
    },
    "tweet_ner7": {
        "group": "social_media",
        "hf_dataset": "tner/tweetner7",
        "config": "tweetner7",
        "output": "tweet_ner7.json",
        "description": "Twitter NER across 7 entity types. Fine-grained social media NER with temporal annotations.",
    },
    "bc5cdr": {
        "group": "biomedical",
        "hf_dataset": "tner/bc5cdr",
        "output": "bc5cdr.json",
        "description": "Biomedical NER for diseases and chemicals. Created for BioCreative V CDR task, a major biomedical NLP benchmark.",
    },
    "ncbidisease": {
        "group": "biomedical",
        "hf_dataset": "ncbi_disease",
        "output": "ncbidisease.json",
        "description": "NCBI disease mentions corpus. Foundational resource for disease NER from NIH.",
    },
    "genia": {
        "group": "biomedical",
        "hf_dataset": "chufangao/GENIA-NER",
        "output": "genia.json",
        "description": "Biomedical NER for molecular biology. First large-scale biomedical NER corpus; historically significant.",
    },
    "anat_em": {
        "group": "biomedical",
        "hf_dataset": "disi-unibo-nlp/AnatEM",
        "output": "anat_em.json",
        "description": "Anatomical entity mention corpus. 1,212 PubMed abstracts with anatomical structures.",
    },
    "bc2gm": {
        "group": "biomedical",
        "hf_dataset": "bigbio/bc2gm_corpus",
        "config": "bigbio_kb",
        "output": "bc2gm.json",
        "description": "BioCreative II Gene Mention recognition. Gold-standard gene/protein name tagging.",
    },
    "bc4chemd": {
        "group": "biomedical",
        "hf_dataset": "bigbio/bc4chemd",
        "config": "bigbio_kb",
        "output": "bc4chemd.json",
        "description": "BioCreative IV Chemical Entity Mention Detection. Drug and chemical name recognition.",
    },
    "gap": {
        "group": "wikipedia",
        "hf_dataset": "google-gap-coreference/gap",
        "output": "gap.json",
        "description": "Gender Ambiguous Pronoun resolution. Google's benchmark for exposing gender bias in coreference systems.",
    },
    "doc_red": {
        "group": "wikipedia",
        "hf_dataset": "docred",
        "output": "doc_red.json",
        "description": "Document-level relation extraction. 96 relation types from Wikipedia.",
    },
    "book_coref": {
        "group": "literature",
        "hf_dataset": "sapienzanlp/bookcoref",
        "output": "book_coref.json",
        "description": "Book-scale coreference. First benchmark with 200k+ tokens/doc average. Character coreference on 53 Project Gutenberg novels.",
    },
    "book_coref_split": {
        "group": "literature",
        "hf_dataset": "sapienzanlp/bookcoref",
        "config": "split",
        "output": "book_coref_split.json",
        "description": "BOOKCOREF split into 1500-token windows for comparison with standard benchmarks.",
    },
    "masakha_ner": {
        "group": "news",
        "hf_dataset": "masakhane/masakhaner",
        "output": "masakha_ner.json",
        "description": "NER for 10 African languages. PER/LOC/ORG/DATE.",
    },
    "masakha_ner2": {
        "group": "news",
        "hf_dataset": "masakhane/masakhaner2",
        "output": "masakha_ner2.json",
        "description": "Extended MasakhaNER with 20+ African languages.",
    },
    "afri_senti": {
        "group": "social_media",
        "hf_dataset": "shmuhammad/AfriSenti-twitter-sentiment",
        "output": "afri_senti.json",
        "description": "Sentiment analysis for 14 African languages. 110k+ tweets. SemEval 2023 Task 12.",
    },
    "afri_qa": {
        "group": "wikipedia",
        "hf_dataset": "masakhane/afriqa",
        "output": "afri_qa.json",
        "description": "Cross-lingual QA for 10 African languages. Wikipedia-based.",
    },
    "masakha_news": {
        "group": "news",
        "hf_dataset": "masakhane/masakhanews",
        "output": "masakha_news.json",
        "description": "News topic classification for 16 African languages.",
    },
    "masakha_pos": {
        "group": "general",
        "hf_dataset": "masakhane/masakhane-pos",
        "output": "masakha_pos.json",
        "description": "Part-of-speech tagging for 20 African languages.",
    },
    "wiki_ann": {
        "group": "wikipedia",
        "hf_dataset": "wikiann",
        "config": "en",
        "output": "wiki_ann.json",
        "description": "Silver-standard NER from Wikipedia hyperlinks. 282 languages.",
    },
    "wiesp2022ner": {
        "group": "scientific",
        "hf_dataset": "adsabs/WIESP2022-NER",
        "output": "wiesp2022ner.json",
        "description": "Astrophysics NER from NASA ADS. 31 entity types: facilities, wavelengths, telescopes, archives.",
    },
    "few_rel": {
        "group": "wikipedia",
        "hf_dataset": "few_rel",
        "output": "few_rel.json",
        "description": "Few-shot relation classification benchmark. 100 relations from Wikidata.",
    },
}

# Statistics: 27 of 431 datasets (6.3%) have HuggingFace IDs
