# Compact Tree Display Spec

## Overview

A tree display mode that reduces vertical space usage through two techniques: collapsing single-child directory chains and displaying files in a horizontal grid layout.

## Feature 1: Single-Child Chain Collapsing (`--collapse-single`)

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
- Collapsed paths use existing directory styling (e.g., trailing slash only if `--classify` is active)
- When `--level` is set, use physical depth (each directory component counts), not display depth

## Feature 2: File Grid Display (`--table-leaves`)

**Behavior**: Render consecutive files within a directory as a horizontal grid, while directories still render as normal tree rows.

**Key change from previous spec**: Directories with mixed content (both subdirs and files) now render subdirs as tree items and group consecutive files into grid rows.

**Before:**
```
project
├── src
│   └── main.rs
├── tests
│   └── test.rs
├── Cargo.toml
├── LICENSE
└── README.md
```

**After (with `--group-directories-first`):**
```
project
├── src
│   └── main.rs
├── tests
│   └── test.rs
└── Cargo.toml LICENSE README.md
```

**After (alphabetical, no dirs-first):**
```
project
├── Cargo.toml LICENSE
├── src
│   └── main.rs
├── tests
│   └── test.rs
└── z-notes.txt
```

**Rules:**
- Directories (including empty ones) always render as individual tree rows
- Consecutive files in sort order are grouped into grid rows
- Each grid uses eza's existing term_grid for layout (reuses grid mode code)
- Grid width is calculated based on available terminal width minus tree indent
- Each grid is laid out independently (no cross-grid column alignment)
- Respect `--group-directories-first` flag for ordering
- If grid wraps to multiple lines, continuation lines get appropriate tree connectors
- The last item(s) in a directory use `└──`, others use `├──`

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
├── Cargo.toml
├── LICENSE
└── README.md
```

**Compact output (with both flags and `--group-directories-first`):**
```
project
├── src/lib/core
│   └── auth.rs config.rs utils.rs
├── tests
│   ├── integration/api
│   │   └── test_auth.rs test_users.rs
│   └── unit
│       └── test_config.rs test_utils.rs
└── Cargo.toml LICENSE README.md
```

## Interaction with Existing Features

- **Colors/icons**: Preserved in grid layout (uses existing eza rendering)
- **Git status**: Shown per-file in grid layout
- **`--long` mode**: Disable file grid display when `--long` is active; chain collapsing still works
- **Hidden files**: Follow existing show/hide setting
- **Depth limit (`--level`)**: Physical depth counting (each directory in chain counts separately)
- **`--group-directories-first`**: Respected; affects where file grids appear in output

## Flags

Two separate flags (no combined `--compact` for now):
- `--collapse-single` for chain collapsing only
- `--table-leaves` for horizontal file grid display only

Both flags require `--tree` mode to be active.

## Implementation Notes

**Reuse existing grid code**: The file grid display should use eza's existing `term_grid` integration (same as grid mode) rather than reimplementing grid layout. This ensures consistent behavior and reduces code duplication.

**No two-pass rendering**: Each directory's file grid is laid out independently based on its content and available width. Column positions may differ between grids at different tree depths or with different content.

**Rendering algorithm for a directory:**
1. Sort entries (respecting `--group-directories-first` if set)
2. Iterate through entries:
   - If directory: render as normal tree row, recurse
   - If file: collect into current file group
   - When hitting a directory or end: flush file group as grid row(s)
3. Grid rows use appropriate tree connectors (├── or └──) based on position

## Required Test Cases

Tests must cover all edge cases using the `trycmd` framework (`tests/cmd/`).

### Chain Collapsing Tests (`--collapse-single`)

1. **Basic chain collapse**: `a/b/c/file.txt` → `a/b/c` with `file.txt` child
2. **Chain stops at multiple children**: `a/b/{c,d}/` → `a/b` with `c`, `d` children
3. **Chain stops at files**: `a/b/file.txt` + `a/b/c/` → `a/b` with both children
4. **Symlink stops chain**: `a/b -> target` should not be collapsed into parent
5. **Empty directory collapse**: `a/b/c/` (all empty) → `a/b/c` with no children (uses existing directory styling)
6. **Mixed depth chains**: Multiple chains at different depths in same tree
7. **`--level` interaction**: `--level 2` with `a/b/c/d/file.txt` should stop at physical depth 2
8. **Root-level single child**: Single directory at root should still show correctly
9. **Chain with `--long`**: Chain collapsing works with detailed output

### File Grid Tests (`--table-leaves`)

1. **Basic file grid**: Directory with only files renders horizontally
2. **Mixed content**: Directory with subdirs and files; files grouped into grid
3. **Dirs-first ordering**: With `--group-directories-first`, files grid at end
4. **Interspersed files**: Without dirs-first, multiple file grids between dirs
5. **Terminal width wrapping**: Wide content wraps to multiple lines correctly
6. **Single file**: Directory with one file (edge case)
7. **Empty directories**: Render as individual rows, not in grid
8. **Continuation line connectors**: Wrapped rows have correct tree connectors
9. **Icons preserved**: `--icons` works with grid layout
10. **Colors preserved**: File type colors work in grid layout
11. **Sorting in grid**: Files sorted correctly within grid
12. **`--long` disables grid**: With `--long`, falls back to vertical list

### Combined Tests (both flags)

1. **Chain + file grid**: Collapsed chain with mixed content at end
2. **Complex tree**: Multiple chains and file grids in one output
3. **Deep nesting**: 5+ level chains with file grids at various depths

### Edge Cases

1. **Empty tree**: Directory with no children
2. **Only symlinks**: Directory containing only symlinks
3. **Hidden files**: `--all` shows hidden files in grids
4. **Very long filenames**: Grid wrapping with long names
5. **Unicode filenames**: Proper width calculation for unicode
