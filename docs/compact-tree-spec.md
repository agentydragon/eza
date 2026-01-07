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

## Feature 2: Compact File Display (`--table-leaves`)

**Behavior**: Display files more compactly using two strategies:

1. **Inline files**: When a directory's contents (files and empty dirs, excluding non-empty subdirs) fit on one line, display them inline with the directory using `─` as separator
2. **Grid files**: Otherwise, render consecutive files as a horizontal grid

**Key principle**: Maximize vertical space savings while maintaining readability.

### Inline Display

When a directory contains only files/empty dirs (no subdirectories with content), and they fit on one line:

**Before:**
```
│   ├── images
│   │   └── screenshots.png
│   ├── tapes
│   │   └── demo.tape
```

**After:**
```
│   ├── images ── screenshots.png
│   ├── tapes ── demo.tape
```

**Multiple files inline:**
```
│   ├── config ── dev.toml prod.toml test.toml
```

**Combined with chain collapsing (`--collapse-single`):**
```
│   ├── src/images ── screenshot.png
```

**Rules for inline display:**
- Applies when directory has no non-empty subdirectories
- All files/empty dirs must fit on remaining line width after directory name
- Uses ` ── ` (space, two box drawing horizontals, space) as separator - same width as tree connectors
- Files are space-separated within the inline display
- If contents don't fit, fall back to normal tree rendering (not grid)

### Grid Display

For directories with mixed content (subdirs and files), files are grouped into grid rows:

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

**Rules for grid display:**
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
├── src/lib/core ── auth.rs config.rs utils.rs
├── tests
│   ├── integration/api ── test_auth.rs test_users.rs
│   └── unit ── test_config.rs test_utils.rs
└── Cargo.toml LICENSE README.md
```

Note: The leaf directories (`core`, `api`, `unit`) use inline display because they contain only files. The root `project` directory uses grid display because it has mixed content (subdirs + files).

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
2. Check if inline display is possible:
   - Are all children files or empty directories (no non-empty subdirs)?
   - Calculate total width: `dir_name + " ── " + space-separated file names`
   - Does it fit in remaining terminal width (after tree indent)?
   - If yes: render as single line `├── dirname ── file1 file2 file3`
3. If not inline, iterate through entries:
   - If directory: render as normal tree row, recurse
   - If file: collect into current file group
   - When hitting a directory or end: flush file group as grid row(s)
4. Grid rows use appropriate tree connectors (├── or └──) based on position

**Inline display rendering:**
- Separator is ` ── ` (space, two box-drawing horizontals U+2500, space) - 4 chars total, matching `TREE_PART_WIDTH`
- Files are separated by single space
- Use existing file rendering (colors, icons) for each file in inline display
- Separator uses tree/punctuation style for consistent appearance with tree connectors

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

### Inline Display Tests (`--table-leaves`)

1. **Basic inline**: Directory with single file → `dir ── file.txt`
2. **Multiple files inline**: Directory with files that fit → `dir ── a.txt b.txt c.txt`
3. **Inline with empty dirs**: Empty subdirs inline with files → `dir ── empty/ file.txt`
4. **Fallback to tree**: Files too wide for line → normal tree rendering
5. **Inline + chain collapse**: `a/b/c` with single file → `a/b/c ── file.txt`
6. **No inline for mixed content**: Directory with non-empty subdir → no inline, use grid/tree
7. **Icons in inline**: `--icons` preserved in inline display
8. **Colors in inline**: File type colors work in inline display

### Grid Display Tests (`--table-leaves`)

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

1. **Chain + inline**: Collapsed chain with files that fit inline
2. **Chain + grid**: Collapsed chain with mixed content at end (uses grid)
3. **Complex tree**: Multiple chains, inline displays, and grids in one output
4. **Deep nesting**: 5+ level chains with inline/grid at various depths

### Edge Cases

1. **Empty tree**: Directory with no children
2. **Only symlinks**: Directory containing only symlinks
3. **Hidden files**: `--all` shows hidden files in grids/inline
4. **Very long filenames**: Fallback behavior with long names
5. **Unicode filenames**: Proper width calculation for unicode
6. **Separator character**: `─` renders correctly in all terminals
