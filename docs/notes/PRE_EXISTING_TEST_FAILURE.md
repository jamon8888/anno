# Pre-Existing Test Failure

**Date**: 2025-01-27  
**Status**: ⚠️ Pre-existing issue (not introduced by current changes)

## Issue

Test failure in `anno-core/src/types/type_label.rs`:

```
test types::type_label::tests::test_serde_roundtrip ... FAILED
assertion `left == right` failed
  left: Custom("PROTEIN")
 right: Core(Custom { name: "GENE", category: Misc })
```

## Analysis

This is a serde roundtrip test that's failing - "PROTEIN" is being deserialized as "GENE" instead. This appears to be a pre-existing issue unrelated to the debugging session changes.

## Impact

- **Scope**: Single test in `anno-core`
- **Severity**: Low (test failure, not compilation error)
- **Related Changes**: None - this file was not modified during debugging session

## Next Steps

1. Investigate serde serialization/deserialization logic in `type_label.rs`
2. Check if there's a mapping issue between "PROTEIN" and "GENE"
3. Fix the roundtrip test or the underlying serde implementation

## Note

This test failure was discovered during final verification but is unrelated to the critical issues resolved in this session:
- ✅ Compilation hang (resolved)
- ✅ Script detection bug (resolved)
- ✅ Semantic chunking errors (resolved)
- ✅ Multilingual tokenizer tests (added and passing)
