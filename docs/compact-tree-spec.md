# Compact Tree Display Spec

## Overview

A tree display mode that reduces vertical space usage through two techniques: collapsing single-child directory chains and displaying leaf nodes in a horizontal table layout.

## Feature 1: Single-Child Chain Collapsing

**Behavior**: When a directory has exactly one child (which is also a directory), collapse the chain into a single path until reaching a directory with multiple children or containing files.

**Before:**
```
└── src
    └── wt
        └── stubs
            └── README.md
```

**After:**
```
└── src/wt/stubs
    └── README.md
```

**Rules:**
- Only collapse directories, not files
- Stop collapsing when a directory has >1 child or contains any files
- Collapsed paths use `/` separator regardless of OS
- Preserve the tree connector (├── or └──) for the collapsed path
- Stop collapsing at symlinks (treat symlinks as non-directories)
- Empty directories are collapsible (single-child with no files)
- Collapsed path ending in empty directory shows trailing slash: `a/b/c/`
- When `--level` is set, use physical depth (each directory component counts), not display depth

## Feature 2: Leaf Table Display

**Behavior**: When displaying the contents of a directory that contains only leaves (no nested structure), render items horizontally in columns like `ls` does, rather than one file per line with tree prefixes.

**Before:**
```
├── tests
│   ├── a.py
│   ├── test_authentication.py
│   ├── test_b.py
│   ├── test_config.py
│   ├── x.py
│   └── z.py
```

**After (correct):**
```
├── tests
│   └── a.py            test_authentication.py  test_b.py
│       test_config.py  x.py                    z.py
```

**Wrong (columns don't align across rows):**
```
├── tests
│   └── a.py  test_authentication.py  test_b.py
│       test_config.py  x.py  z.py
```

**Rules:**
- Only applies to directories containing exclusively files (no subdirectories)
- Preserve tree connector (└──) before the first file in the table
- Respect terminal width for column wrapping
- If table wraps to multiple lines, continuation lines align with first filename (after the connector)
- Each column has fixed width based on the longest entry in that column (columns may have different widths from each other)
- Column start positions (character indices) must be consistent across all tables in the output — not just within a single directory's table
- Indentation aligns with parent directory's content area
- Sorting follows existing tree sort order (alphabetical, by type, etc.)

## Combined Example

**Input structure:**
```
project/
├── src/
│   └── lib/
│       └── core/
│           ├── auth.rs
│           ├── config.rs
│           └── utils.rs
├── tests/
│   ├── integration/
│   │   └── api/
│   │       ├── test_auth.rs
│   │       └── test_users.rs
│   └── unit/
│       ├── test_config.rs
│       └── test_utils.rs
└── docs/
    ├── README.md
    └── CHANGELOG.md
```

**Compact output (correct):**
```
project
├── src/lib/core
│   └── auth.rs         config.rs   utils.rs
├── tests
│   ├── integration/api
│   │   └── test_auth.rs   test_users.rs
│   └── unit
│       └── test_config.rs  test_utils.rs
└── docs
    └── CHANGELOG.md    README.md
```

Note: Column start positions are consistent across all tables (e.g., column 2 always starts at the same character index relative to the table's indentation level).

**Wrong (inconsistent column positions across tables):**
```
project
├── src/lib/core
│   └── auth.rs  config.rs  utils.rs
├── tests
│   ├── integration/api
│   │   └── test_auth.rs  test_users.rs
│   └── unit
│       └── test_config.rs  test_utils.rs
└── docs
    └── CHANGELOG.md  README.md
```

## Interaction with Existing Features

- **Colors/icons**: Preserved in table layout (uses existing eza icon rendering)
- **Git status**: Shown per-file in table layout
- **`--long` mode**: Disable leaf table display when `--long` is active; chain collapsing still works
- **Hidden files**: Follow existing show/hide setting
- **Depth limit (`--level`)**: Physical depth counting (each directory in chain counts separately)

## Flags

Two separate flags (no combined `--compact` for now):
- `--collapse-single` for chain collapsing only
- `--table-leaves` for horizontal leaf display only

Both flags require `--tree` mode to be active.

## Implementation Notes

This feature should be implementable by combining eza's existing modes:
- **Tree mode**: Already handles directory traversal, connectors, and indentation
- **Grid/table mode**: Already handles column width calculation and alignment

The hybrid mode reuses tree rendering for structure and grid rendering for leaf directories. Should work with eza's existing icon support.

**Two-pass rendering**: Required for consistent column alignment across all leaf tables:
1. **Pass 1**: Walk the entire tree to collect all filenames from leaf directories
2. Calculate global column widths based on longest filename per column across ALL leaf tables
3. **Pass 2**: Render the tree using pre-calculated column widths

## Required Test Cases

Tests must cover all edge cases using the `trycmd` framework (`tests/cmd/`).

### Chain Collapsing Tests (`--collapse-single`)

1. **Basic chain collapse**: `a/b/c/file.txt` → `a/b/c` with `file.txt` child
2. **Chain stops at multiple children**: `a/b/{c,d}/` → `a/b` with `c`, `d` children
3. **Chain stops at files**: `a/b/file.txt` + `a/b/c/` → `a/b` with both children
4. **Symlink stops chain**: `a/b -> target` should not be collapsed into parent
5. **Empty directory collapse**: `a/b/c/` (all empty) → `a/b/c/` with no children (trailing slash indicates empty)
6. **Mixed depth chains**: Multiple chains at different depths in same tree
7. **`--level` interaction**: `--level 2` with `a/b/c/d/file.txt` should stop at physical depth 2
8. **Root-level single child**: Single directory at root should still show correctly
9. **Chain with `--long`**: Chain collapsing works with detailed output

### Leaf Table Tests (`--table-leaves`)

1. **Basic leaf table**: Directory with only files renders horizontally
2. **Column alignment across tables**: Multiple leaf directories have consistent column positions
3. **Terminal width wrapping**: Wide content wraps to multiple lines correctly
4. **Single file leaf**: Directory with one file (edge case for table layout)
5. **Mixed leaf/non-leaf siblings**: Some dirs are leaves, some have subdirs
6. **Continuation line alignment**: Wrapped rows align with first filename
7. **Icons preserved**: `--icons` works with leaf table layout
8. **Colors preserved**: File type colors work in table layout
9. **Sorting in table**: Files sorted correctly within table layout
10. **`--long` disables table**: With `--long`, falls back to vertical list

### Combined Tests (both flags)

1. **Chain + leaf table**: Collapsed chain ending in leaf directory
2. **Complex tree**: Multiple chains and leaf tables in one output
3. **Deep nesting**: 5+ level chains with leaf tables at various depths

### Edge Cases

1. **Empty tree**: Directory with no children
2. **Only symlinks**: Directory containing only symlinks
3. **Hidden files**: `--all` shows hidden files in leaf tables
4. **Very long filenames**: Column width calculation with long names
5. **Unicode filenames**: Proper width calculation for unicode
