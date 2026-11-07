//! Round-trip SSH config parser and editor used by Hop.
//!
//! The parser keeps every byte of the original file. Edits replace only the lines of the block
//! being changed, so comments, blank lines, indentation and order elsewhere are never touched.

mod document;
mod edit;
mod load;
mod meta;
mod name;
mod notes_io;
mod resolve;
mod save;

pub use document::{Block, BlockKind, ConfigOption, Document, LineEnding};
pub use edit::{EditError, HostInput};
pub use load::{load, ConfigSet, LoadedFile};
pub use meta::{Forward, HostMeta};
pub use name::is_valid_host_name;
pub use notes_io::{
	apply_import, export_notes, merge_meta, parse_notes, preview_import, to_json, ForwardEntry,
	ImportPreview, ListMode, NoteEntry, NotesError, NotesFile, MAX_NOTES_FILE_BYTES, NOTES_VERSION,
};
pub use resolve::{parse_ssh_g, resolve, ResolveError, ResolvedHost};
pub use save::{write_with_backup, BACKUPS_KEPT};
