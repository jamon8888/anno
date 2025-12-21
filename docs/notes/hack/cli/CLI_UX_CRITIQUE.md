# CLI UX/Design Critique

**Last Updated**: Based on codebase review + clap best practices + MCP research

## Executive Summary

The `anno` CLI has solid foundations but suffers from inconsistent patterns, discoverability issues, and workflow friction. The core functionality is well-designed, but the user experience could be significantly improved through better consistency, clearer error messages, and more intuitive command structures.

**Key Finding**: The codebase has a `get_input_text()` helper function that standardizes input handling, but it's not consistently used across all commands. This is a quick win for consistency.

## Critical Issues

### 1. Inconsistent Input Methods Across Commands

**Problem**: Different commands handle input differently:
- `extract`: `--text`, `--file`, or positional args (uses `get_input_text()`)
- `debug`: `--text`, `--file` (no positional, doesn't use `get_input_text()`)
- `eval`: `--text`, `--file`, or positional args (uses `get_input_text()`)
- `cross-doc`: Only directory path (positional, different pattern entirely)

**Root Cause**: `get_input_text()` helper exists at line 2881 but `debug` command doesn't use it.

**Impact**: Users must remember different patterns for each command. Breaks muscle memory.

**Recommendation**: 
1. **Quick fix**: Make `debug` use `get_input_text()` to support positional args
2. **Better fix**: Standardize all single-document commands:
```bash
# All single-doc commands support same pattern
anno extract "text"              # positional
anno extract --text "text"       # explicit flag
anno extract --file doc.txt      # file input
anno extract < doc.txt           # stdin

anno debug "text"                 # should work but doesn't
anno debug --text "text"          # works
anno debug --file doc.txt         # works
```

**Code Location**: `src/bin/anno.rs:856` - `cmd_debug` doesn't pass `&[]` to `get_input_text()`

**Priority**: High (quick win - just fix `debug` to use helper)

### 2. Model Backend Discoverability

**Problem**: Users must know model names exist, and feature-gated models fail silently or with unclear errors.

**Current behavior**:
- `--model gliner` fails if `onnx` feature not enabled
- No clear indication of which models are available
- Default is `stacked` but help text doesn't explain what it does
- `anno info` shows models but doesn't indicate which are actually available

**Root Cause**: `cmd_info()` at line 2277 lists all models but doesn't check feature flags dynamically.

**Recommendation**:
1. **Enhance `anno info`**: Show which models are actually available (not just listed)
2. **Add `anno models` subcommand**:
```bash
anno models list          # Show available models with status
anno models info gliner    # Show model details, requirements, performance
anno models compare        # Compare available models side-by-side
```

3. **Better error messages** (clap can help here):
```bash
anno extract --model gliner
# Error: Model 'gliner' requires 'onnx' feature.
# 
# Available models:
#   ✓ pattern, heuristic, stacked (always available)
#   ✗ gliner, gliner2, nuner, w2ner (requires --features onnx)
# 
# Build with: cargo build --features onnx
# Or use: anno models list to see all options
```

**Code Location**: `src/bin/anno.rs:2277` - `cmd_info()` should use `available_backends()` from `src/lib.rs:686`

**Priority**: High

### 3. Output Format Inconsistency

**Problem**: Different commands use different format flags:
- `extract`: `--format <FORMAT>` (human, json, jsonl, tsv, inline, grounded)
- `cross-doc`: `--format <FORMAT>` (json, tree, jsonl, summary)
- `debug`: `--html` (boolean flag, not format enum)
- `eval`: `--json` and `--html` (boolean flags)

**Impact**: Users can't predict which format options exist for each command.

**Recommendation**: Standardize format handling:
```bash
# Option 1: Unified format enum
anno extract --format json
anno debug --format html
anno cross-doc --format tree

# Option 2: Consistent boolean flags
anno extract --json
anno debug --html
anno cross-doc --tree
```

**Priority**: Medium

### 4. Error Message Quality

**Problem**: Error messages are inconsistent and sometimes unhelpful.

**Examples**:
```bash
# Current
anno extract --model invalid
# error: invalid value 'invalid' for '--model <MODEL>'

# Better
anno extract --model invalid
# Error: Unknown model 'invalid'
# Available models: pattern, heuristic, stacked, gliner, ...
# Use 'anno models list' to see all available models
```

**Recommendation**: 
- Always suggest alternatives
- Include context (what was attempted, what's available)
- Use consistent error formatting

**Priority**: High

### 5. Cross-Doc Command Name Inconsistency

**Problem**: Command is `cross-doc` but help text shows `crossdoc` in examples.

**Current**:
```bash
anno cross-doc --help
# Examples show: anno crossdoc /path/to/documents
```

**Recommendation**: Fix examples to match actual command name.

**Priority**: Low (cosmetic)

## Design Issues

### 6. Verbose/Quiet Flag Confusion

**Problem**: `--verbose` and `--quiet` have different meanings across commands:
- `extract --verbose`: Show context around entities
- `extract --quiet`: Minimal output
- `cross-doc --verbose`: Show progress
- `debug --quiet`: Suppress status messages

**Impact**: Users can't predict what verbose/quiet will do.

**Recommendation**: 
- Standardize meanings: `--verbose` = more detail, `--quiet` = less output
- Document what each level shows
- Consider `--progress` flag for cross-doc instead of `--verbose`

**Priority**: Medium

### 7. Missing Pipeline Integration

**Problem**: No way to chain commands or reuse results.

**Current workflow**:
```bash
# Extract entities
anno extract --file doc.txt --format json > doc.json

# Can't use doc.json in cross-doc
anno cross-doc /path/to/docs  # Re-extracts everything
```

**Recommendation**: Add import/export:
```bash
# Export extract results
anno extract --file doc.txt --export doc.grounded.json

# Use in cross-doc
anno cross-doc --import doc1.grounded.json doc2.grounded.json
```

**Priority**: Medium (already documented as TODO)

### 8. Help Text Quality

**Problem**: Help text is comprehensive but overwhelming.

**Current**: Long help text with all options, but hard to scan.

**Recommendation**: 
- Add "Quick Start" section to main help
- Use `--help` for full docs, `-h` for summary
- Add examples to each command's help

**Priority**: Medium

### 9. Default Behavior Unclear

**Problem**: Defaults aren't obvious:
- What does `stacked` model do?
- What's the difference between `human` and `inline` format?
- What threshold should I use for cross-doc?

**Recommendation**:
- Add `--help-defaults` flag showing all defaults
- Explain defaults in help text
- Add `anno config show` to show current configuration

**Priority**: Low

### 10. Feature Flag Discovery

**Problem**: Users don't know which features are enabled.

**Current**: Must try commands and see if they fail.

**Recommendation**:
```bash
anno info  # Should show enabled features
# Output:
# Features: cli, onnx, eval-advanced
# Available models: pattern, heuristic, stacked, gliner, gliner2, ...
```

**Priority**: Medium

## Workflow Issues

### 11. No Interactive Mode

**Problem**: All commands are one-shot. No REPL or interactive exploration.

**Recommendation**: Consider adding:
```bash
anno interactive
# Enter interactive mode with:
# - Command history
# - Tab completion
# - Result caching
```

**Priority**: Low (nice-to-have)

### 12. Progress Feedback

**Problem**: Long-running operations (cross-doc, benchmark) don't show progress clearly.

**Current**: `--verbose` shows some progress, but not structured.

**Recommendation**:
```bash
# Show progress bar for long operations
anno cross-doc /path/to/docs
# Processing: [████████████░░░░░░░░] 60% (120/200 files)
```

**Priority**: Low

### 13. Output Redirection Confusion

**Problem**: Some commands write to stdout, others to stderr, mixing is confusing.

**Current**:
- Results → stdout
- Progress → stderr (sometimes)
- Errors → stderr

**Recommendation**: 
- Document what goes where
- Add `--output` flag consistently
- Consider `--log` flag for progress/debug info

**Priority**: Low

## Positive Aspects

### ✅ Good Command Aliases
- `x` for extract, `d` for debug, `cd` for cross-doc
- Short and memorable

### ✅ Comprehensive Help Text
- Long help text is detailed and informative
- Examples are helpful

### ✅ Flexible Input Methods
- Multiple ways to provide input (text, file, positional)
- Good for different use cases

### ✅ Consistent Error Handling
- All commands return proper exit codes
- Errors are formatted consistently

### ✅ Feature Gating
- Advanced features properly gated
- Prevents confusion about unavailable features

## Recommendations Summary

### High Priority (Quick Wins)
1. **Fix `debug` command** to use `get_input_text()` for positional args (1-line change)
2. **Enhance `anno info`** to show actually available models using `available_backends()`
3. **Better error messages** with suggestions (clap provides this, just need to use it)
4. **Fix cross-doc examples** in help text to match actual command name

### Medium Priority (Architectural)
5. **Standardize output formats** (unified format enum across all commands)
6. **Clarify verbose/quiet** meanings across commands (document in help)
7. **Add pipeline integration** (import/export GroundedDocument JSON)
8. **Improve help text** structure (quick start section, examples per command)
9. **Add `anno models` subcommand** for model discovery and comparison

### Low Priority (Nice-to-Have)
10. **Add interactive mode** for exploration (REPL-style)
11. **Better progress indicators** for long operations (cross-doc, benchmark)
12. **Document output streams** (stdout vs stderr) in help text
13. **Add `--version` subcommand** instead of flag (clap best practice)

## Implementation Quick Wins

### 1. Fix Debug Command (5 minutes)
```rust
// src/bin/anno.rs:856
// Change from:
let text = get_input_text(&args.text, args.file.as_deref(), &[])?;
// To:
let text = get_input_text(&args.text, args.file.as_deref(), &args.positional)?;
```

### 2. Enhance Info Command (15 minutes)
```rust
// src/bin/anno.rs:2277
// Add to cmd_info():
println!("{}:", color("1;33", "Available Models (this build)"));
let backends = anno::available_backends();
for (name, available) in backends {
    let status = if available { "✓" } else { "✗" };
    println!("  {} {} {}", status, name, 
        if available { "" } else { "(requires feature flag)" });
}
```

### 3. Better Error Messages (leverage clap)
Clap 4.x provides excellent error messages out of the box. The current errors are already good, but we can enhance them by:
- Using `clap::error::Error::format()` for consistent formatting
- Adding `suggestions` to error context when models aren't available

## Implementation Notes

### Quick Wins (Minutes to Hours)
1. **Fix `debug` command** (1 line): `src/bin/anno.rs:856` - add `&args.positional` parameter
2. **Enhance `info` command** (15 lines): Use `anno::available_backends()` to show actual availability
3. **Fix cross-doc examples** in help text: Update examples to use `cross-doc` not `crossdoc`
4. **Standardize error message format**: Leverage clap's built-in error formatting

### Medium Effort (Hours to Days)
5. **Add `anno models` subcommand**: New subcommand with list/info/compare actions
6. **Standardize input methods**: Ensure all commands use `get_input_text()` helper
7. **Unify output format handling**: Create shared format enum, migrate all commands
8. **Improve help text structure**: Add quick start section, examples per command

### Larger Changes (Days to Weeks)
9. **Add pipeline integration**: Import/export GroundedDocument JSON format
10. **Add interactive mode**: REPL-style interface for exploration
11. **Add progress bars**: For long operations (cross-doc, benchmark)
12. **Refactor command structure**: Consider merging `anno-eval` into `anno` as subcommands

## Code Locations Reference

- **Input text helper**: `src/bin/anno.rs:2881` - `get_input_text()` (not used by debug)
- **Extract command**: `src/bin/anno.rs:658` - `cmd_extract()` (uses helper correctly)
- **Debug command**: `src/bin/anno.rs:830` - `cmd_debug()` (missing positional args)
- **Eval command**: `src/bin/anno.rs:962` - `cmd_eval()` (uses helper correctly)
- **Cross-doc command**: `src/bin/anno.rs:2312` - `cmd_crossdoc()` (different pattern)
- **Info command**: `src/bin/anno.rs:2277` - `cmd_info()` (should use `available_backends()`)
- **Model availability**: `src/lib.rs:686` - `available_backends()` (not used by info)
- **GroundedDocument**: `src/grounded.rs` - Core data structure for pipeline
- **CDCR logic**: `src/eval/cdcr.rs` - Cross-document clustering implementation

