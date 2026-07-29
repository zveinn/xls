mod columns;
mod entry;
mod format;
mod sort;
mod sys;

use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use columns::Column;
use entry::Entry;
use format::{
    DIM, GREEN, HIDDEN_DIR, HIDDEN_EXEC, HIDDEN_FILE, HIDDEN_LINK, LIGHT_BLUE, ORANGE, RED, RESET,
    WHITE, Widths, write_entry, write_header,
};
use sort::sort_entries;

enum Cli {
    Help,
    List {
        path: PathBuf,
        columns: Vec<Column>,
        sort: Option<Column>,
        headers: bool,
    },
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match parse_args(&args) {
        Ok(Cli::Help) => {
            print_help();
            ExitCode::SUCCESS
        }
        Ok(Cli::List {
            path,
            columns,
            sort,
            headers,
        }) => {
            if let Err(e) = run(&path, &columns, sort, headers) {
                eprintln!("{RED}xls: {e}{RESET}");
                return ExitCode::FAILURE;
            }
            ExitCode::SUCCESS
        }
        Err(msg) => {
            eprintln!("{RED}xls: {msg}{RESET}");
            eprintln!("Try '{WHITE}xls --help{RESET}' for more information.");
            ExitCode::FAILURE
        }
    }
}

fn parse_args(args: &[String]) -> Result<Cli, String> {
    let mut path = None;
    let mut help = false;
    let mut sort = None;
    let mut headers = true;
    let mut columns = None;
    let mut all = false;
    let mut i = 0;

    while i < args.len() {
        let a = args[i].as_str();
        match a {
            "-h" | "--help" => help = true,
            "--noHeaders" | "--no-headers" => headers = false,
            "--all" => all = true,
            "--columns" => {
                i += 1;
                let Some(list) = args.get(i) else {
                    return Err(
                        "--columns requires a comma-separated list (e.g. --columns MTIME,NAME)"
                            .into(),
                    );
                };
                columns = Some(Column::parse_list(list)?);
            }
            s if let Some(list) = s.strip_prefix("--columns=") => {
                if list.is_empty() {
                    return Err(
                        "--columns requires a comma-separated list (e.g. --columns=MTIME,NAME)"
                            .into(),
                    );
                }
                columns = Some(Column::parse_list(list)?);
            }
            "--sort" => {
                i += 1;
                let Some(field) = args.get(i) else {
                    return Err("--sort requires a column name (e.g. --sort MTIME)".into());
                };
                sort = Some(Column::parse(field)?);
            }
            s if let Some(field) = s.strip_prefix("--sort=") => {
                if field.is_empty() {
                    return Err("--sort requires a column name (e.g. --sort=MTIME)".into());
                }
                sort = Some(Column::parse(field)?);
            }
            s if s.starts_with('-') => return Err(format!("unknown flag {s}")),
            s => {
                if path.is_some() {
                    return Err("only one path is supported".into());
                }
                path = Some(PathBuf::from(s));
            }
        }
        i += 1;
    }

    if help {
        return Ok(Cli::Help);
    }

    if all && columns.is_some() {
        return Err("use either --all or --columns, not both".into());
    }

    let columns = if all {
        Column::all()
    } else {
        columns.unwrap_or_else(Column::defaults)
    };

    Ok(Cli::List {
        path: path.unwrap_or_else(|| PathBuf::from(".")),
        columns,
        sort,
        headers,
    })
}

fn print_help() {
    let h = LIGHT_BLUE;
    let k = WHITE;
    let d = DIM;
    let o = ORANGE;
    let fields = Column::names().join(", ");
    let defaults = Column::defaults()
        .iter()
        .map(|c| c.header())
        .collect::<Vec<_>>()
        .join(",");

    println!(
        "\
{h}xls{RESET} — colored directory listing

{h}USAGE{RESET}
  {k}xls{RESET} [{k}--all{RESET}|{k}--columns{RESET} {k}COLS{RESET}] [{k}--sort{RESET} {k}COL{RESET}] [{k}--noHeaders{RESET}] [{k}path{RESET}]
  {k}xls{RESET} [{k}-h{RESET}|{k}--help{RESET}]

{h}OPTIONS{RESET}
  {k}--all{RESET}             Show every column in a sensible order
  {k}--columns{RESET} {k}COLS{RESET}   Comma-separated columns to show, in order
  {k}--sort{RESET} {k}COL{RESET}       Sort by column (always ascending)
  {k}--noHeaders{RESET}      Do not print the column header row
  {k}-h{RESET}, {k}--help{RESET}      Show this help and exit

{h}COLUMNS{RESET}
  Default ({k}--columns{RESET} / {k}--all{RESET} omitted):
    {k}{defaults}{RESET}

  {k}--all{RESET} order:
    {k}MTIME,PERMS,N,USER,GROUP,SIZE,BLOCKS,S,INO:IGEN,DEV,ATIME,CTIME,BIRTH,FLAGS,XATTRS,XFS,NAME{RESET}

  Available:
    {k}{fields}{RESET}

  Examples:
    {k}xls --all{RESET}
    {k}xls --columns NAME,SIZE{RESET}
    {k}xls --columns=MTIME,PERMS,USER,GROUP,SIZE,NAME{RESET}
    {k}xls --columns MTIME,NAME,XFS --sort SIZE{RESET}

{h}SORTING{RESET}
  Use {k}--sort COL{RESET} or {k}--sort=COL{RESET}. Names are case-insensitive.
  Order is always {o}ascending{RESET} (smallest / oldest / A–Z first).
  Ties break on {k}NAME{RESET} ascending.
  You may sort by a column that is not displayed.

  Notes:
    {k}SIZE{RESET}, {k}N{RESET}, {k}BLOCKS{RESET}, {k}INO:IGEN{RESET}, {k}DEV{RESET}  numeric (low → high)
    {k}MTIME{RESET}, {k}ATIME{RESET}, {k}CTIME{RESET}, {k}BIRTH{RESET}   oldest first
    {k}NAME{RESET}, {k}USER{RESET}, {k}GROUP{RESET}, {k}PERMS{RESET}     lexicographic A–Z
    Aliases: {d}NLINK/LINKS{RESET}→N, {d}INODE{RESET}→INO:IGEN, {d}OWNER{RESET}→USER, …

{h}COLORS{RESET}
  {WHITE}white{RESET}        regular file
  {LIGHT_BLUE}light blue{RESET}   directory (and headers)
  {GREEN}green{RESET}        executable
  {o}orange{RESET}       symlink / special file
  {RED}red{RESET}          error or broken symlink
  {HIDDEN_FILE}cyan{RESET}         hidden file ({d}.name{RESET})
  {HIDDEN_DIR}violet{RESET}       hidden directory ({d}.dir{RESET})
  {HIDDEN_EXEC}yellow{RESET}       hidden executable
  {HIDDEN_LINK}pink{RESET}         hidden symlink / special

{h}COLUMN REFERENCE{RESET}
  {k}MTIME{RESET}     Last content modification time (UTC, DD-MM-YYYY HH:MM:SS)
  {k}PERMS{RESET}     Classic mode string (color-coded), e.g. {d}drwxr-xr-x{RESET}
  {k}USER{RESET}      Owner user name
  {k}GROUP{RESET}     Owner group name
  {k}SIZE{RESET}      Logical size (human-readable: B/K/M/G/T)
  {k}NAME{RESET}      Entry name (color indicates type); symlinks show {d}->{RESET} target
  {k}N{RESET}         Hard link count
  {k}BLOCKS{RESET}    Allocated blocks and I/O block size ({d}<st_blocks>b/<blksize>{RESET})
  {k}S{RESET}         Sparse flag: {d}S{RESET} if allocated < size, else {d}-{RESET}
  {k}INO:IGEN{RESET}  Inode number and generation (when available)
  {k}DEV{RESET}       Device id ({d}major:minor{RESET}); devices also show {d}rdev{RESET}
  {k}ATIME{RESET}     Last access time (may be stale on noatime mounts)
  {k}CTIME{RESET}     Last status-change time (metadata change, not create)
  {k}BIRTH{RESET}     Creation / birth time when the filesystem provides it
  {k}FLAGS{RESET}     Linux inode flags from {d}FS_IOC_GETFLAGS{RESET}, or {d}-{RESET}
  {k}XATTRS{RESET}    Extended attribute names, comma-separated, or {d}-{RESET}
  {k}XFS{RESET}       Cheap XFS info ({d}FS_IOC_FSGETXATTR{RESET} / {d}DIOINFO{RESET}):
                    xflags, exts, proj, esz, cow, dio — or {d}-{RESET} if unavailable

{h}EXAMPLES{RESET}
  {k}xls{RESET}
  {k}xls /var/log{RESET}
  {k}xls --all{RESET}
  {k}xls --columns NAME,SIZE,MTIME{RESET}
  {k}xls --columns NAME,XFS --sort XFS{RESET}
  {k}xls --sort SIZE --noHeaders{RESET}
"
    );
}

fn run(
    path: &Path,
    columns: &[Column],
    sort: Option<Column>,
    headers: bool,
) -> io::Result<()> {
    let mut detail = Column::max_detail(columns);
    if let Some(key) = sort {
        detail = detail.max(key.min_detail());
    }

    let meta = fs::symlink_metadata(path)?;

    let mut entries = if meta.is_dir() {
        let mut v = Vec::new();
        for ent in fs::read_dir(path)? {
            let ent = ent?;
            let name = ent.file_name().to_string_lossy().into_owned();
            match Entry::collect(ent.path(), name, detail) {
                Ok(e) => v.push(e),
                Err(err) => eprintln!("{RED}xls: {}: {err}{RESET}", ent.path().display()),
            }
        }
        v
    } else {
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        vec![Entry::collect(path.to_path_buf(), name, detail)?]
    };

    match sort {
        Some(key) => sort_entries(&mut entries, key),
        None => entries.sort_by(|a, b| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
        }),
    }

    let widths = Widths::measure(&entries, columns);
    let mut out = io::stdout().lock();
    if headers {
        write_header(&mut out, columns, &widths)?;
    }
    for e in &entries {
        write_entry(&mut out, e, columns, &widths)?;
    }
    out.flush()
}
