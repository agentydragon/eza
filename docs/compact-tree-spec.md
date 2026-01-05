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

## Implementation Notes

This feature should be implementable by combining eza's existing modes:
- **Tree mode**: Already handles directory traversal, connectors, and indentation
- **Grid/table mode**: Already handles column width calculation and alignment

The hybrid mode reuses tree rendering for structure and grid rendering for leaf directories. Should work with eza's existing icon support.

## Interaction with Existing Features

- **Colors/icons**: Preserved in table layout (uses existing eza icon rendering)
- **Git status**: Shown per-file in table layout
- **File size/permissions**: If enabled, probably disable table layout (too complex) or show as separate column per file
- **Hidden files**: Follow existing show/hide setting
- **Depth limit**: Collapsing counts as traversal, not display

## Suggested Flag

`--compact` or `-C` to enable both features together. Could also be split:
- `--collapse-single` for chain collapsing only
- `--table-leaves` for horizontal leaf display only
