# Dataset URL Health Report

**Date**: 2025-01-27  
**Source**: `scripts/check_url_health.sh`  
**Total URLs Checked**: 688

## Summary

| Status | Count | Percentage |
|--------|-------|------------|
| **Valid** | 545 | 79.2% |
| **Broken** | 94 | 13.7% |
| **Auth Required** | 49 | 7.1% |
| **Timeout/Error** | 0 | 0.0% |

## Broken URLs (94)

### GitHub Repositories (Most Common Issue)

Many GitHub repos have moved, been deleted, or require authentication:

- `https://github.com/THUDM/Tem-DocRED`
- `https://github.com/allenai/scico-radar`
- `https://github.com/astronomical-ner/AstroNER`
- `https://github.com/Liquid-Legal-Institute/LegalBench`
- `https://github.com/allenai/sciner`
- `https://github.com/techner/techner`
- `https://github.com/character-codex/character-codex`
- `https://github.com/gun-violence-corpus/gvc`
- `https://github.com/cltl/FCC`
- `https://github.com/sopan-sarkar/multiparty-dialogue-coref`
- `https://github.com/UniversalAnaphora/UA-CODI-CRAC`
- `https://github.com/mixred/MixRED`
- `https://github.com/covered/CovEReD`
- `https://github.com/allenai/sciie`
- `https://github.com/EDS-NLP/eds-nlp`
- `https://github.com/fintech-patent-ner`
- `https://github.com/wateragriner`
- `https://github.com/food-ner/social`
- `https://github.com/russian-cultural-ner`
- `https://github.com/spanish-medieval-nlp`
- `https://github.com/czech-medieval-charters`
- `https://github.com/guarani-nlp`
- `https://github.com/ixa-ehu/shipibo-konibo`
- `https://github.com/navajo-nlp`
- `https://github.com/cltl/OpenBoek`
- `https://github.com/allenai/quoref`
- `https://github.com/mpsilfern/finer`
- `https://github.com/legal-ner/legal-ner`
- `https://github.com/Stardust-hyx/CEREC`
- `https://github.com/delicate-nlp/delicate`
- `https://github.com/rsttools/signal`
- `https://github.com/btime-bert/bitimebert`
- `https://github.com/tridis/tridis`
- `https://github.com/queerbench/queerbench`
- `https://github.com/queereotypes/queereotypes`
- `https://github.com/medical-annotation-pipeline/map`
- `https://github.com/homomex/homomex`
- `https://github.com/ener-dataset/ener`

### Other Broken URLs

- `https://universaldependencies.org/treebanks/eo_pud/index.html`
- `https://wiki.dothraki.org/High_Valyrian`
- `https://www.cs.york.ac.uk/semeval-2013/task9/`
- `https://cs.nyu.edu/~davise/papers/WinoPron/`
- `https://www.semantic-web-journal.net/content/greek-mythology-knowledge-graph`
- `https://ramprasad.mse.gatech.edu/PolyIE/`
- `https://2023.eswc-conferences.org/AgriNER/`
- `http://www.cs.toronto.edu/~varada/ASN/`

### Error Responses

- `https://www.linguateca.pt/HAREM/` - ERROR (000000) - Connection/timeout issue
- `https://ritual.uh.edu/lince/` - ERROR (000000) - Connection/timeout issue
- `https://chemdataextractor.org/` - ERROR (000000) - Connection/timeout issue
- `https://eldamo.org/` - ERROR (406) - Not Acceptable
- `https://www.aclweb.org/anthology/S14-2004/` - ERROR (409) - Conflict

## Auth Required (49)

These URLs require authentication or are behind paywalls:

- `https://pmc.ncbi.nlm.nih.gov/articles/PMC12048500/`
- `https://research.tue.nl/files/349781334/978-3-031-61057-8_9.pdf`
- `https://academic.oup.com/database/article/doi/10.1093/database/bax087/4621360`
- `https://academic.oup.com/jamia/article/18/5/552/830538`
- `https://www.clips.uantwerpen.be/conll2002/ner/data/ned.testa`
- `https://www.clips.uantwerpen.be/conll2002/ner/data/esp.testa`
- `https://biocreative.bioinformatics.udel.edu/resources/biocreative-ii-corpus/`
- `https://pmc.ncbi.nlm.nih.gov/articles/PMC7592802/`
- `https://direct.mit.edu/coli/article/49/3/703/116160`
- `https://direct.mit.edu/tacl/article/doi/10.1162/tacl_a_00581/117162`
- `https://direct.mit.edu/coli/article/50/3/953/121178`
- `https://academic.oup.com/database/article/doi/10.1093/database/baac098/6833204`
- `https://pmc.ncbi.nlm.nih.gov/articles/PMC10869356/`
- `https://www.sciencedirect.com/science/article/abs/pii/S0957417423024272`
- `https://www.sciencedirect.com/science/article/abs/pii/S0098300425000949`
- `https://www.sciencedirect.com/science/article/pii/S266729522300082X`
- `https://arc.aiaa.org/doi/10.2514/1.I011251`
- `https://biocreative.bioinformatics.udel.edu/tasks/biocreative-vii/track-1/`
- `https://www.sciencedirect.com/science/article/abs/pii/S0957417423009429`
- `https://dl.acm.org/doi/10.1145/3743047`
- `https://www.raa-journal.org/issues/all/2024/v24n6/202405/`
- `https://agupubs.onlinelibrary.wiley.com/doi/abs/10.1029/2019EA000610`
- `https://www.sciencedirect.com/science/article/pii/S0926580520309481`
- `https://learnnavi.org/`

## Recommendations

### Immediate Actions

1. **GitHub Repos**: Check if repos moved to different organizations or were renamed
2. **Broken Links**: Update to current URLs or mark as deprecated
3. **Auth Required**: Document that these require authentication/access
4. **Error Responses**: Investigate server issues or update URLs

### Long-term Improvements

1. **Mirror URLs**: Add mirror URLs for critical datasets
2. **HuggingFace Migration**: Many GitHub repos may have moved to HuggingFace
3. **URL Validation**: Add periodic health checks to CI/CD
4. **Access Status**: Update `access_status()` method to reflect current reality

### Priority Fixes

**High Priority** (Core benchmarks):
- CoNLL-2002 URLs (auth required - may need alternative)
- OntoNotes URLs (verify accessibility)
- MultiCoNER (GitHub → check HuggingFace)

**Medium Priority** (Domain-specific):
- Biomedical NER datasets (many behind paywalls - document)
- Legal NER (GitHub repos - check alternatives)

**Low Priority** (Experimental/niche):
- Constructed languages (Klingon, Toki Pona)
- Historical/classical language datasets

## Next Steps

1. Create script to update broken GitHub URLs to HuggingFace equivalents
2. Document auth requirements in dataset metadata
3. Add `mirror_url` for critical datasets
4. Update `access_status()` based on health check results
