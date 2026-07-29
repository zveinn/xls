//! Descending sort by listing header field.

use std::cmp::Ordering;
use std::time::SystemTime;

use crate::entry::Entry;

/// A column / header we can sort on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Perms,
    Size,
    Mtime,
    Name,
    Nlink,
    User,
    Group,
    Blocks,
    Sparse,
    Ino,
    Dev,
    Atime,
    Ctime,
    Birth,
    Flags,
    Xattrs,
    Xfs,
}

impl SortKey {
    /// Parse a header name (case-insensitive). Accepts common aliases.
    pub fn parse(s: &str) -> Result<Self, String> {
        let key = match s.trim().to_ascii_uppercase().as_str() {
            "PERMS" | "MODE" | "PERMISSIONS" => Self::Perms,
            "SIZE" => Self::Size,
            "MTIME" | "MODIFIED" => Self::Mtime,
            "NAME" => Self::Name,
            "N" | "NLINK" | "LINKS" => Self::Nlink,
            "USER" | "OWNER" | "UID" => Self::User,
            "GROUP" | "GID" => Self::Group,
            "BLOCKS" | "ALLOC" => Self::Blocks,
            "S" | "SPARSE" => Self::Sparse,
            "INO" | "INODE" | "INO:IGEN" | "IGEN" => Self::Ino,
            "DEV" | "DEVICE" => Self::Dev,
            "ATIME" | "ACCESSED" => Self::Atime,
            "CTIME" | "CHANGED" => Self::Ctime,
            "BIRTH" | "BTIME" | "CREATED" => Self::Birth,
            "FLAGS" | "FL" => Self::Flags,
            "XATTRS" | "XA" | "XATTR" => Self::Xattrs,
            "XFS" => Self::Xfs,
            other => {
                return Err(format!(
                    "unknown sort field '{other}' (try: {})",
                    Self::names().join(", ")
                ));
            }
        };
        Ok(key)
    }

    /// Header-style names for help / error messages.
    pub fn names() -> &'static [&'static str] {
        &[
            "PERMS", "SIZE", "MTIME", "NAME", "N", "USER", "GROUP", "BLOCKS", "S", "INO:IGEN",
            "DEV", "ATIME", "CTIME", "BIRTH", "FLAGS", "XATTRS", "XFS",
        ]
    }

    /// Minimum collection detail needed to sort meaningfully.
    /// 0 = basic stat, 1 = portable extras, 2 = + XFS.
    pub fn min_detail(self) -> u8 {
        match self {
            Self::Perms | Self::Size | Self::Mtime | Self::Name => 0,
            Self::Xfs => 2,
            _ => 1,
        }
    }
}

/// Sort `entries` by `key` in **descending** order.
/// Ties break on name ascending (case-insensitive).
pub fn sort_entries(entries: &mut [Entry], key: SortKey) {
    entries.sort_by(|a, b| {
        cmp_desc(a, b, key).then_with(|| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
        })
    });
}

fn cmp_desc(a: &Entry, b: &Entry, key: SortKey) -> Ordering {
    // Build ascending ordering, then reverse → descending.
    let asc = match key {
        SortKey::Perms => a.mode.cmp(&b.mode),
        SortKey::Size => a.size.cmp(&b.size),
        SortKey::Mtime => cmp_time(a.mtime, b.mtime),
        SortKey::Name => a
            .name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase()),
        SortKey::Nlink => a.nlink.cmp(&b.nlink),
        SortKey::User => cmp_str_ci(&a.user, &b.user),
        SortKey::Group => cmp_str_ci(&a.group, &b.group),
        SortKey::Blocks => a
            .blocks
            .cmp(&b.blocks)
            .then_with(|| a.blksize.cmp(&b.blksize)),
        SortKey::Sparse => a.sparse.cmp(&b.sparse),
        SortKey::Ino => a
            .ino
            .cmp(&b.ino)
            .then_with(|| cmp_opt_u32(a.extras.inode_gen, b.extras.inode_gen)),
        SortKey::Dev => a
            .dev_major
            .cmp(&b.dev_major)
            .then_with(|| a.dev_minor.cmp(&b.dev_minor))
            .then_with(|| a.rdev_major.cmp(&b.rdev_major))
            .then_with(|| a.rdev_minor.cmp(&b.rdev_minor)),
        SortKey::Atime => cmp_time(a.atime, b.atime),
        SortKey::Ctime => a.ctime_secs.cmp(&b.ctime_secs),
        SortKey::Birth => cmp_time(a.birth, b.birth),
        SortKey::Flags => cmp_str_ci(&flags_key(a), &flags_key(b)),
        SortKey::Xattrs => a
            .extras
            .xattrs
            .len()
            .cmp(&b.extras.xattrs.len())
            .then_with(|| cmp_str_ci(&a.extras.xattrs.join(","), &b.extras.xattrs.join(","))),
        SortKey::Xfs => cmp_xfs(a, b),
    };
    asc.reverse()
}

fn cmp_str_ci(a: &str, b: &str) -> Ordering {
    a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase())
}

fn cmp_time(a: Option<SystemTime>, b: Option<SystemTime>) -> Ordering {
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less, // missing counts as oldest
        (Some(_), None) => Ordering::Greater,
        (Some(a), Some(b)) => a.cmp(&b),
    }
}

fn cmp_opt_u32(a: Option<u32>, b: Option<u32>) -> Ordering {
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(a), Some(b)) => a.cmp(&b),
    }
}

fn flags_key(e: &Entry) -> String {
    e.extras.flags.join(",")
}

fn cmp_xfs(a: &Entry, b: &Entry) -> Ordering {
    match (a.xfs(), b.xfs()) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(ax), Some(bx)) => ax
            .nextents
            .cmp(&bx.nextents)
            .then_with(|| ax.projid.cmp(&bx.projid))
            .then_with(|| ax.extsize.cmp(&bx.extsize))
            .then_with(|| ax.cowextsize.cmp(&bx.cowextsize))
            .then_with(|| ax.xflags.join(",").cmp(&bx.xflags.join(","))),
    }
}
