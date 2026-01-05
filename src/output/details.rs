// SPDX-FileCopyrightText: 2024 Christina Sørensen
// SPDX-License-Identifier: EUPL-1.2
//
// SPDX-FileCopyrightText: 2023-2024 Christina Sørensen, eza contributors
// SPDX-FileCopyrightText: 2014 Benjamin Sago
// SPDX-License-Identifier: MIT
//! The **Details** output view displays each file as a row in a table.
//!
//! It’s used in the following situations:
//!
//! - Most commonly, when using the `--long` command-line argument to display the
//!   details of each file, which requires using a table view to hold all the data;
//! - When using the `--tree` argument, which uses the same table view to display
//!   each file on its own line, with the table providing the tree characters;
//! - When using both the `--long` and `--grid` arguments, which constructs a
//!   series of tables to fit all the data on the screen.
//!
//! You will probably recognise it from the `ls --long` command. It looks like
//! this:
//!
//! ```text
//!     .rw-r--r--  9.6k ben 29 Jun 16:16 Cargo.lock
//!     .rw-r--r--   547 ben 23 Jun 10:54 Cargo.toml
//!     .rw-r--r--  1.1k ben 23 Nov  2014 LICENCE
//!     .rw-r--r--  2.5k ben 21 May 14:38 README.md
//!     .rw-r--r--  382k ben  8 Jun 21:00 screenshot.png
//!     drwxr-xr-x     - ben 29 Jun 14:50 src
//!     drwxr-xr-x     - ben 28 Jun 19:53 target
//! ```
//!
//! The table is constructed by creating a `Table` value, which produces a `Row`
//! value for each file. These rows can contain a vector of `Cell`s, or they can
//! contain depth information for the tree view, or both. These are described
//! below.
//!
//!
//! ## Constructing Detail Views
//!
//! When using the `--long` command-line argument, the details of each file are
//! displayed next to its name.
//!
//! The table holds a vector of all the column types. For each file and column, a
//! `Cell` value containing the ANSI-coloured text and Unicode width of each cell
//! is generated, with the row and column determined by indexing into both arrays.
//!
//! The column types vector does not actually include the filename. This is
//! because the filename is always the rightmost field, and as such, it does not
//! need to have its width queried or be padded with spaces.
//!
//! To illustrate the above:
//!
//! ```text
//!     ┌─────────────────────────────────────────────────────────────────────────┐
//!     │ columns: [ Permissions,  Size,   User,  Date(Modified) ]                │
//!     ├─────────────────────────────────────────────────────────────────────────┤
//!     │   rows:  cells:                                            filename:    │
//!     │   row 1: [ ".rw-r--r--", "9.6k", "ben", "29 Jun 16:16" ]   Cargo.lock   │
//!     │   row 2: [ ".rw-r--r--",  "547", "ben", "23 Jun 10:54" ]   Cargo.toml   │
//!     │   row 3: [ "drwxr-xr-x",    "-", "ben", "29 Jun 14:50" ]   src          │
//!     │   row 4: [ "drwxr-xr-x",    "-", "ben", "28 Jun 19:53" ]   target       │
//!     └─────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! Each column in the table needs to be resized to fit its widest argument. This
//! means that we must wait until every row has been added to the table before it
//! can be displayed, in order to make sure that every column is wide enough.

use std::io::{self, Write};
use std::path::PathBuf;
use std::vec::IntoIter as VecIntoIter;

use nu_ansi_term::Style;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use log::{debug, trace};

use crate::fs::dir_action::RecurseOptions;
use crate::fs::feature::git::GitCache;
use crate::fs::feature::xattr::Attribute;
use crate::fs::fields::SecurityContextType;
use crate::fs::filter::FileFilter;
use crate::fs::{Dir, File};
use crate::output::cell::TextCell;
use crate::output::color_scale::{ColorScaleInformation, ColorScaleOptions};
use crate::output::file_name::Options as FileStyle;
use crate::output::table::{Options as TableOptions, Row as TableRow, Table};
use crate::output::tree::{TreeDepth, TreeParams, TreeTrunk};
use crate::theme::Theme;

/// Information about a collapsed directory chain.
/// When --collapse-single is active and we have a chain like a/b/c where each
/// directory has exactly one child directory, we collapse it into "a/b/c".
struct CollapsedChain {
    /// The names of directories in the collapsed chain (e.g., ["a", "b", "c"])
    names: Vec<String>,
    /// The final Dir at the end of the chain (c's contents)
    final_dir: Option<Dir>,
    /// Total depth traversed (for --level checking)
    depth_traversed: usize,
}

/// Configuration for rendering leaf directories as horizontal tables.
/// Used when --table-leaves is active.
#[derive(Debug, Clone)]
struct LeafTableConfig {
    /// Width of each column in the grid layout.
    /// All leaf tables share the same column widths for alignment.
    column_widths: Vec<usize>,
    /// Minimum spacing between columns.
    column_spacing: usize,
}

impl LeafTableConfig {
    /// Calculate column widths from all leaf directory contents.
    /// This ensures consistent column positions across all leaf tables.
    fn from_leaf_files(all_leaf_files: &[Vec<usize>], available_width: usize) -> Self {
        let column_spacing = 2; // Standard 2-space gap between columns

        if all_leaf_files.is_empty() {
            return Self {
                column_widths: vec![],
                column_spacing,
            };
        }

        // Find the maximum number of files in any leaf directory
        let max_files = all_leaf_files.iter().map(|f| f.len()).max().unwrap_or(0);
        if max_files == 0 {
            return Self {
                column_widths: vec![],
                column_spacing,
            };
        }

        // Calculate how many columns can fit
        // Start with the widest single file to determine minimum column width
        let max_single_width = all_leaf_files
            .iter()
            .flat_map(|files| files.iter())
            .max()
            .copied()
            .unwrap_or(1);

        // Determine number of columns that fit
        let mut num_cols = 1;
        let mut test_cols = 2;
        while test_cols <= max_files {
            // Calculate total width needed for test_cols columns
            let total_width = max_single_width * test_cols + column_spacing * (test_cols - 1);
            if total_width <= available_width {
                num_cols = test_cols;
                test_cols += 1;
            } else {
                break;
            }
        }

        // Now calculate actual column widths based on position
        // Column i holds items at positions i, i+num_cols, i+2*num_cols, etc.
        let mut column_widths = vec![0usize; num_cols];
        for files in all_leaf_files {
            for (idx, &width) in files.iter().enumerate() {
                let col = idx % num_cols;
                column_widths[col] = column_widths[col].max(width);
            }
        }

        Self {
            column_widths,
            column_spacing,
        }
    }

    /// Get the total width of one grid row.
    /// Currently unused but kept for potential future use in width calculations.
    #[allow(dead_code)]
    fn row_width(&self) -> usize {
        if self.column_widths.is_empty() {
            0
        } else {
            self.column_widths.iter().sum::<usize>()
                + self.column_spacing * (self.column_widths.len() - 1)
        }
    }
}

/// With the **Details** view, the output gets formatted into columns, with
/// each `Column` object showing some piece of information about the file,
/// such as its size, or its permissions.
///
/// To do this, the results have to be written to a table, instead of
/// displaying each file immediately. Then, the width of each column can be
/// calculated based on the individual results, and the fields are padded
/// during output.
///
/// Almost all the heavy lifting is done in a Table object, which handles the
/// columns for each row.
#[allow(clippy::struct_excessive_bools)]
/// This clearly isn't a state machine
#[derive(PartialEq, Eq, Debug)]
pub struct Options {
    /// Options specific to drawing a table.
    ///
    /// Directories themselves can pick which columns are *added* to this
    /// list, such as the Git column.
    pub table: Option<TableOptions>,

    /// Whether to show a header line or not.
    pub header: bool,

    /// Whether to show each file’s extended attributes.
    pub xattr: bool,

    /// Whether to show each file's security attribute.
    pub secattr: bool,

    /// Whether to show a directory's mounted filesystem details
    pub mounts: bool,

    pub color_scale: ColorScaleOptions,

    /// Whether to drill down into symbolic links that point to directories
    pub follow_links: bool,
}

pub struct Render<'a> {
    pub dir: Option<&'a Dir>,
    pub files: Vec<File<'a>>,
    pub theme: &'a Theme,
    pub file_style: &'a FileStyle,
    pub opts: &'a Options,

    /// Whether to recurse through directories with a tree view, and if so,
    /// which options to use. This field is only relevant here if the `tree`
    /// field of the `RecurseOptions` is `true`.
    pub recurse: Option<RecurseOptions>,

    /// How to sort and filter the files after getting their details.
    pub filter: &'a FileFilter,

    /// Whether we are skipping Git-ignored files.
    pub git_ignoring: bool,

    pub git: Option<&'a GitCache>,

    pub git_repos: bool,
}

#[rustfmt::skip]
struct Egg<'a> {
    table_row: Option<TableRow>,
    xattrs:    &'a [Attribute],
    errors:    Vec<(io::Error, Option<PathBuf>)>,
    dir:       Option<Dir>,
    file:      &'a File<'a>,
}

impl<'a> AsRef<File<'a>> for Egg<'a> {
    fn as_ref(&self) -> &File<'a> {
        self.file
    }
}

impl<'a> Render<'a> {
    pub fn render<W: Write>(mut self, w: &mut W) -> io::Result<()> {
        let mut rows = Vec::new();

        let color_scale_info = ColorScaleInformation::from_color_scale(
            self.opts.color_scale,
            &self.files,
            self.filter.dot_filter,
            self.git,
            self.git_ignoring,
            self.recurse,
        );

        // First pass: collect leaf file widths for --table-leaves (if enabled)
        // This allows consistent column alignment across all leaf tables
        // Table leaves is disabled in --long mode (when table is present)
        let table_leaves_active = self.recurse.map(|r| r.table_leaves).unwrap_or(false)
            && self.opts.table.is_none();
        let leaf_config = if table_leaves_active {
            let leaf_widths = self.collect_leaf_file_widths(&self.files, TreeDepth::root());
            // Use 80 as default available width; actual tree indent will reduce this
            let available_width = 60; // Conservative estimate for leaf table content
            Some(LeafTableConfig::from_leaf_files(&leaf_widths, available_width))
        } else {
            None
        };

        if let Some(ref table) = self.opts.table {
            match (self.git, self.dir) {
                (Some(g), Some(d)) => {
                    if !g.has_anything_for(&d.path) {
                        self.git = None;
                    }
                }
                (Some(g), None) => {
                    if !self.files.iter().any(|f| g.has_anything_for(&f.path)) {
                        self.git = None;
                    }
                }
                (None, _) => { /* Keep Git how it is */ }
            }

            let mut table = Table::new(table, self.git, self.theme, self.git_repos);

            if self.opts.header {
                let header = table.header_row();
                table.add_widths(&header);
                rows.push(self.render_header(header));
            }

            // This is weird, but I can't find a way around it:
            // https://internals.rust-lang.org/t/should-option-mut-t-implement-copy/3715/6
            let mut table = Some(table);
            self.add_files_to_table(
                &mut table,
                &mut rows,
                &self.files,
                TreeDepth::root(),
                color_scale_info,
                leaf_config.as_ref(),
            );

            for row in self.iterate_with_table(table.unwrap(), rows) {
                writeln!(w, "{}", row.strings())?;
            }
        } else {
            self.add_files_to_table(
                &mut None,
                &mut rows,
                &self.files,
                TreeDepth::root(),
                color_scale_info,
                leaf_config.as_ref(),
            );

            for row in self.iterate(rows) {
                writeln!(w, "{}", row.strings())?;
            }
        }

        Ok(())
    }

    /// Whether to show the extended attribute hint
    pub fn show_xattr_hint(&self, file: &File<'_>) -> bool {
        // Do not show the hint '@' if the only extended attribute is the security
        // attribute and the security attribute column is active.
        let xattr_count = file.extended_attributes().len();
        let selinux_ctx_shown = self.opts.secattr
            && match file.security_context().context {
                SecurityContextType::SELinux(_) => true,
                SecurityContextType::None => false,
            };
        xattr_count > 1 || (xattr_count == 1 && !selinux_ctx_shown)
    }

    /// Adds files to the table, possibly recursively. This is easily
    /// parallelisable, and uses a pool of threads.
    fn add_files_to_table<'dir>(
        &self,
        table: &mut Option<Table<'a>>,
        rows: &mut Vec<Row>,
        src: &[File<'dir>],
        depth: TreeDepth,
        color_scale_info: Option<ColorScaleInformation>,
        leaf_config: Option<&LeafTableConfig>,
    ) {
        use crate::fs::feature::xattr;

        let mut file_eggs: Vec<_> = src
            .par_iter()
            .map(|file| {
                let mut errors = Vec::new();

                // There are three “levels” of extended attribute support:
                //
                // 1. If we’re compiling without that feature, then
                //    exa pretends all files have no attributes.
                // 2. If the feature is enabled and the --extended flag
                //    has been specified, then display an @ in the
                //    permissions column for files with attributes, the
                //    names of all attributes and their values, and any
                //    errors encountered when getting them.
                // 3. If the --extended flag *hasn’t* been specified, then
                //    display the @, but don’t display anything else.
                //
                // For a while, exa took a stricter approach to (3):
                // if an error occurred while checking a file’s xattrs to
                // see if it should display the @, exa would display that
                // error even though the attributes weren’t actually being
                // shown! This was confusing, as users were being shown
                // errors for something they didn’t explicitly ask for,
                // and just cluttered up the output. So now errors aren’t
                // printed unless the user passes --extended to signify
                // that they want to see them.

                let xattrs: &[Attribute] = if xattr::ENABLED && self.opts.xattr {
                    file.extended_attributes()
                } else {
                    &[]
                };

                let table_row = table
                    .as_ref()
                    .map(|t| t.row_for_file(file, self.show_xattr_hint(file), color_scale_info));

                let mut dir = None;
                let follow_links = self.opts.follow_links;
                if let Some(r) = self.recurse {
                    if (if follow_links {
                        file.points_to_directory()
                    } else {
                        file.is_directory()
                    }) && r.tree
                        && !r.is_too_deep(depth.0)
                    {
                        trace!("matching on read_dir");
                        match file.read_dir() {
                            Ok(d) => {
                                dir = Some(d);
                            }
                            Err(e) => {
                                errors.push((e, None));
                            }
                        }
                    }
                }

                Egg {
                    table_row,
                    xattrs,
                    errors,
                    dir,
                    file,
                }
            })
            .collect();

        // this is safe because all entries have been initialized above
        self.filter.sort_files(&mut file_eggs);

        for (tree_params, egg) in depth.iterate_over(file_eggs.into_iter()) {
            let mut files = Vec::new();
            let errors = egg.errors;

            if let (Some(ref mut t), Some(row)) = (table.as_mut(), egg.table_row.as_ref()) {
                t.add_widths(row);
            }

            // Check if collapse_single is enabled
            let collapse_enabled = self
                .recurse
                .map(|r| r.collapse_single)
                .unwrap_or(false);

            // Try to collapse single-child directory chains if enabled
            let (file_name, effective_dir, _extra_depth) = if let Some(dir) = egg.dir {
                if collapse_enabled {
                    // Try to collapse this directory chain
                    let chain = self.try_collapse_chain(egg.file.name.clone(), dir, depth.0);
                    if chain.names.len() > 1 {
                        // We have a collapsed chain - render it specially
                        let collapsed_name = self.render_collapsed_name(&chain, egg.file);
                        debug!("collapsed chain: {:?}", chain.names);
                        (collapsed_name, chain.final_dir, chain.depth_traversed - 1)
                    } else {
                        // No collapsing happened, render normally
                        let name = self
                            .file_style
                            .for_file(egg.file, self.theme)
                            .with_link_paths()
                            .with_mount_details(self.opts.mounts)
                            .paint()
                            .promote();
                        (name, chain.final_dir, 0)
                    }
                } else {
                    // collapse_single not enabled, render normally
                    let name = self
                        .file_style
                        .for_file(egg.file, self.theme)
                        .with_link_paths()
                        .with_mount_details(self.opts.mounts)
                        .paint()
                        .promote();
                    (name, Some(dir), 0)
                }
            } else {
                // Not a directory, render normally
                let name = self
                    .file_style
                    .for_file(egg.file, self.theme)
                    .with_link_paths()
                    .with_mount_details(self.opts.mounts)
                    .paint()
                    .promote();
                (name, None, 0)
            };

            debug!("file_name {file_name:?}");

            let row = Row {
                tree: tree_params,
                cells: egg.table_row,
                name: file_name,
            };

            rows.push(row);

            if let Some(ref dir) = effective_dir {
                for file_to_add in dir.files(
                    self.filter.dot_filter,
                    self.git,
                    self.git_ignoring,
                    egg.file.deref_links,
                    egg.file.is_recursive_size(),
                ) {
                    files.push(file_to_add);
                }

                self.filter
                    .filter_child_files(self.recurse.is_some(), &mut files);

                if !files.is_empty() {
                    // For visual tree depth, use depth.deeper() (one level down from current)
                    // The extra_depth from collapsed chains is only tracked for potential
                    // future --level checking, but visual rendering stays at depth+1
                    let visual_depth = depth.deeper();

                    for xattr in egg.xattrs {
                        rows.push(self.render_xattr(xattr, TreeParams::new(visual_depth, false)));
                    }

                    for (error, path) in errors {
                        rows.push(self.render_error(
                            &error,
                            TreeParams::new(visual_depth, false),
                            path,
                        ));
                    }

                    // Check if this is a leaf directory and --table-leaves is enabled
                    let table_leaves_enabled = self
                        .recurse
                        .map(|r| r.table_leaves)
                        .unwrap_or(false);

                    if table_leaves_enabled
                        && leaf_config.is_some()
                        && self.is_leaf_directory(&files)
                    {
                        // Render leaf directory as horizontal table
                        self.filter.sort_files(&mut files);
                        let config = leaf_config.unwrap();

                        // Split files into rows based on column count
                        let num_cols = config.column_widths.len().max(1);
                        let mut file_idx = 0;

                        while file_idx < files.len() {
                            let row_files: Vec<_> = files
                                .iter()
                                .skip(file_idx)
                                .take(num_cols)
                                .collect();

                            let _is_first_row = file_idx == 0;
                            let is_last_row = file_idx + num_cols >= files.len();

                            // Create the grid row content
                            let grid_content =
                                self.render_leaf_grid_row(&row_files, config);

                            // Create row with appropriate tree params
                            let tree_params = TreeParams::new(visual_depth, is_last_row);
                            let row = Row {
                                tree: tree_params,
                                cells: None, // No table cells for leaf grid
                                name: grid_content,
                            };
                            rows.push(row);

                            file_idx += num_cols;
                        }
                    } else {
                        // Normal recursive rendering
                        self.add_files_to_table(
                            table,
                            rows,
                            &files,
                            visual_depth,
                            color_scale_info,
                            leaf_config,
                        );
                    }
                    continue;
                }
            }

            let count = egg.xattrs.len();
            for (index, xattr) in egg.xattrs.iter().enumerate() {
                let params =
                    TreeParams::new(depth.deeper(), errors.is_empty() && index == count - 1);
                let r = self.render_xattr(xattr, params);
                rows.push(r);
            }

            let count = errors.len();
            for (index, (error, path)) in errors.into_iter().enumerate() {
                let params = TreeParams::new(depth.deeper(), index == count - 1);
                let r = self.render_error(&error, params, path);
                rows.push(r);
            }
        }
    }

    #[must_use]
    pub fn render_header(&self, header: TableRow) -> Row {
        Row {
            tree: TreeParams::new(TreeDepth::root(), false),
            cells: Some(header),
            name: TextCell::paint_str(self.theme.ui.header.unwrap_or_default(), "Name"),
        }
    }

    fn render_error(&self, error: &io::Error, tree: TreeParams, path: Option<PathBuf>) -> Row {
        use crate::output::file_name::Colours;

        let error_message = if let Some(path) = path {
            format!("<{}: {}>", path.display(), error)
        } else {
            format!("<{error}>")
        };

        // TODO: broken_symlink() doesn’t quite seem like the right name for
        // the style that’s being used here. Maybe split it in two?
        let name = TextCell::paint(self.theme.broken_symlink(), error_message);
        Row {
            cells: None,
            name,
            tree,
        }
    }

    fn render_xattr(&self, xattr: &Attribute, tree: TreeParams) -> Row {
        let name = TextCell::paint(
            self.theme.ui.perms.unwrap_or_default().attribute(),
            format!("{xattr}"),
        );
        Row {
            cells: None,
            name,
            tree,
        }
    }

    /// Builds a collapsed chain starting from the given directory.
    /// Called when collapse_single is enabled. Will collapse single-child directory
    /// chains like a/b/c where each has exactly one child directory.
    fn try_collapse_chain(
        &self,
        starting_name: String,
        starting_dir: Dir,
        current_depth: usize,
    ) -> CollapsedChain {
        let recurse = match self.recurse {
            Some(r) => r,
            None => {
                return CollapsedChain {
                    names: vec![starting_name],
                    final_dir: Some(starting_dir),
                    depth_traversed: 1,
                }
            }
        };

        let mut names = vec![starting_name];
        let mut current_dir = starting_dir;
        let mut depth_traversed = 1;

        loop {
            // Check if we've hit the depth limit
            if recurse.is_too_deep(current_depth + depth_traversed) {
                break;
            }

            // Get the files in this directory
            let files: Vec<File<'_>> = current_dir
                .files(
                    self.filter.dot_filter,
                    self.git,
                    self.git_ignoring,
                    false, // deref_links
                    false, // total_size
                )
                .collect();

            // Apply filters
            let mut filtered_files = files;
            self.filter
                .filter_child_files(self.recurse.is_some(), &mut filtered_files);

            // Check if we have exactly one child
            if filtered_files.len() != 1 {
                break;
            }

            let only_child = &filtered_files[0];

            // Check if the only child is a directory (not a symlink, not a file)
            // Symlinks should stop the chain per spec
            if only_child.is_link() || !only_child.is_directory() {
                break;
            }

            // Try to read the child directory
            let child_dir = match only_child.read_dir() {
                Ok(d) => d,
                Err(_) => break,
            };

            // Add this directory name to the chain
            names.push(only_child.name.clone());
            depth_traversed += 1;
            current_dir = child_dir;
        }

        CollapsedChain {
            names,
            final_dir: Some(current_dir),
            depth_traversed,
        }
    }

    /// Renders a collapsed chain name like "a/b/c" with directory styling.
    fn render_collapsed_name(&self, chain: &CollapsedChain, file: &File<'_>) -> TextCell {
        use nu_ansi_term::AnsiString;
        use unicode_width::UnicodeWidthStr;

        let dir_style = self
            .theme
            .ui
            .filekinds
            .as_ref()
            .map(|fk| fk.directory())
            .unwrap_or_default();

        // Build the collapsed path string with "/" separators
        // If the file has no parent_dir (i.e., it's a command-line argument),
        // we need to include the path prefix to match normal rendering
        let collapsed_suffix = chain.names[1..].join("/");
        let collapsed_path = if file.parent_dir.is_none() {
            // Include the original file's path (which may include directory prefix)
            if collapsed_suffix.is_empty() {
                file.path.display().to_string()
            } else {
                format!("{}/{}", file.path.display(), collapsed_suffix)
            }
        } else {
            // File is in a directory, use just the names
            chain.names.join("/")
        };

        // Check if we should add icons
        let mut bits: Vec<AnsiString<'_>> = Vec::new();
        let mut total_width: usize = 0;

        // Add icon if enabled (use the first directory's icon)
        if let crate::output::file_name::ShowIcons::Always(spaces)
        | crate::output::file_name::ShowIcons::Automatic(spaces) = self.file_style.show_icons
        {
            if self.file_style.is_a_tty
                || matches!(
                    self.file_style.show_icons,
                    crate::output::file_name::ShowIcons::Always(_)
                )
            {
                let icon = crate::output::icons::icon_for_file(file);
                let icon_style = crate::output::icons::iconify_style(dir_style);
                let icon_str = icon.to_string();
                let spaces_str = " ".repeat(spaces as usize);
                total_width += icon_str.width() + spaces_str.width();
                bits.push(icon_style.paint(icon_str));
                bits.push(icon_style.paint(spaces_str));
            }
        }

        total_width += collapsed_path.width();
        bits.push(dir_style.paint(collapsed_path));

        // Add classify suffix if enabled
        if let crate::output::file_name::Classify::AddFileIndicators
        | crate::output::file_name::Classify::AutomaticAddFileIndicators = self.file_style.classify
        {
            if self.file_style.is_a_tty
                || matches!(
                    self.file_style.classify,
                    crate::output::file_name::Classify::AddFileIndicators
                )
            {
                total_width += 1;
                bits.push(Style::default().paint("/"));
            }
        }

        TextCell {
            contents: bits.into(),
            width: crate::output::cell::DisplayWidth::from(total_width),
        }
    }

    /// Check if a list of files represents a "leaf" directory - one containing
    /// only files (no subdirectories). Used for --table-leaves rendering.
    fn is_leaf_directory(&self, files: &[File<'_>]) -> bool {
        // A leaf directory has at least one file and no directories
        !files.is_empty()
            && !files.iter().any(|f| {
                // Check if it's a directory (but not a symlink to a directory)
                f.is_directory() && !f.is_link()
            })
    }

    /// Render a leaf directory's files as a horizontal grid row.
    /// Returns a TextCell containing all files formatted in columns.
    /// Currently unused but kept for potential alternative rendering paths.
    #[allow(dead_code)]
    fn render_leaf_table_row(
        &self,
        files: &[File<'_>],
        config: &LeafTableConfig,
        _first_row: bool,
    ) -> TextCell {
        let mut result = TextCell::default();

        for (idx, file) in files.iter().enumerate() {
            // Get the rendered file name
            let file_name = self.file_style.for_file(file, self.theme).paint();
            let name_width = *file_name.width();

            // Append the file name
            result.append(file_name.promote());

            // Add padding to reach column width (except for last item in row)
            let col = idx % config.column_widths.len().max(1);
            let col_width = config.column_widths.get(col).copied().unwrap_or(name_width);

            if idx < files.len() - 1 {
                let padding = col_width.saturating_sub(name_width) + config.column_spacing;
                if padding > 0 {
                    result.add_spaces(padding);
                }
            }
        }

        result
    }

    /// Render a row of the leaf grid (used when files are split across multiple rows).
    fn render_leaf_grid_row(&self, files: &[&File<'_>], config: &LeafTableConfig) -> TextCell {
        let mut result = TextCell::default();

        for (idx, file) in files.iter().enumerate() {
            // Get the rendered file name
            let file_name = self.file_style.for_file(file, self.theme).paint();
            let name_width = *file_name.width();

            // Append the file name
            result.append(file_name.promote());

            // Add padding to reach column width (except for last item in row)
            let col = idx % config.column_widths.len().max(1);
            let col_width = config.column_widths.get(col).copied().unwrap_or(name_width);

            if idx < files.len() - 1 {
                let padding = col_width.saturating_sub(name_width) + config.column_spacing;
                if padding > 0 {
                    result.add_spaces(padding);
                }
            }
        }

        result
    }

    /// Collect file name widths from all leaf directories for global column calculation.
    /// This is the first pass of two-pass rendering for --table-leaves.
    fn collect_leaf_file_widths<'dir>(
        &self,
        src: &[File<'dir>],
        depth: TreeDepth,
    ) -> Vec<Vec<usize>> {
        let mut all_widths = Vec::new();

        let recurse = match self.recurse {
            Some(r) if r.table_leaves => r,
            _ => return all_widths,
        };

        for file in src {
            if !file.is_directory() || file.is_link() {
                continue;
            }

            if recurse.is_too_deep(depth.0) {
                continue;
            }

            // Try to read the directory
            let dir = match file.read_dir() {
                Ok(d) => d,
                Err(_) => continue,
            };

            // Get files in this directory
            let mut files: Vec<File<'_>> = dir
                .files(
                    self.filter.dot_filter,
                    self.git,
                    self.git_ignoring,
                    file.deref_links,
                    file.is_recursive_size(),
                )
                .collect();

            self.filter
                .filter_child_files(self.recurse.is_some(), &mut files);

            if self.is_leaf_directory(&files) {
                // Collect widths for this leaf directory
                let widths: Vec<usize> = files
                    .iter()
                    .map(|f| {
                        let name = self.file_style.for_file(f, self.theme).paint();
                        *name.width()
                    })
                    .collect();
                all_widths.push(widths);
            } else {
                // Recurse into subdirectories
                let sub_widths = self.collect_leaf_file_widths(&files, depth.deeper());
                all_widths.extend(sub_widths);
            }
        }

        all_widths
    }

    #[must_use]
    pub fn iterate_with_table(&'a self, table: Table<'a>, rows: Vec<Row>) -> TableIter<'a> {
        TableIter {
            tree_trunk: TreeTrunk::default(),
            total_width: table.widths().total(),
            table,
            inner: rows.into_iter(),
            tree_style: self.theme.ui.punctuation.unwrap_or_default(),
        }
    }

    #[must_use]
    pub fn iterate(&'a self, rows: Vec<Row>) -> Iter {
        Iter {
            tree_trunk: TreeTrunk::default(),
            inner: rows.into_iter(),
            tree_style: self.theme.ui.punctuation.unwrap_or_default(),
        }
    }
}

pub struct Row {
    /// Vector of cells to display.
    ///
    /// Most of the rows will be used to display files’ metadata, so this will
    /// almost always be `Some`, containing a vector of cells. It will only be
    /// `None` for a row displaying an attribute or error, neither of which
    /// have cells.
    pub cells: Option<TableRow>,

    /// This file’s name, in coloured output. The name is treated separately
    /// from the other cells, as it never requires padding.
    pub name: TextCell,

    /// Information used to determine which symbols to display in a tree.
    pub tree: TreeParams,
}

#[rustfmt::skip]
pub struct TableIter<'a> {
    inner: VecIntoIter<Row>,
    table: Table<'a>,

    total_width: usize,
    tree_style:  Style,
    tree_trunk:  TreeTrunk,
}

impl Iterator for TableIter<'_> {
    type Item = TextCell;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|row| {
            let mut cell = if let Some(cells) = row.cells {
                self.table.render(cells)
            } else {
                let mut cell = TextCell::default();
                cell.add_spaces(self.total_width);
                cell
            };

            for tree_part in self.tree_trunk.new_row(row.tree) {
                cell.push(self.tree_style.paint(tree_part.ascii_art()), 4);
            }

            cell.append(row.name);
            cell
        })
    }
}

pub struct Iter {
    tree_trunk: TreeTrunk,
    tree_style: Style,
    inner: VecIntoIter<Row>,
}

impl Iterator for Iter {
    type Item = TextCell;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|row| {
            let mut cell = TextCell::default();

            for tree_part in self.tree_trunk.new_row(row.tree) {
                cell.push(self.tree_style.paint(tree_part.ascii_art()), 4);
            }

            cell.append(row.name);
            cell
        })
    }
}
