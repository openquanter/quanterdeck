//! The trading process's control port, as the agent reaches it.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use oq_deck_core::lang::Said;
use serde_json::Value;

/// The one control socket in `dir`.
///
/// # Errors
/// None there (the trader is not running, or has no port), or more than
/// one — which would mean two traders, and a guess about which to command
/// is not one to make.
pub fn socket_in(dir: &Path) -> Result<PathBuf, Said> {
    use std::os::unix::fs::FileTypeExt;
    let socks: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| Said::same(format!("{}: {e}", dir.display())))?
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_name().to_string_lossy().ends_with(".sock")
                && std::fs::symlink_metadata(e.path()).is_ok_and(|m| m.file_type().is_socket())
        })
        .map(|e| e.path())
        .collect();
    match socks.len() {
        0 => Err(Said::new(
            "交易进程没有控制端口：它没有在运行，或者启动时没带。",
            "the trader has no control socket: it is not running, or runs without one",
        )),
        1 => Ok(socks.into_iter().next().unwrap_or_default()),
        n => Err(Said::new(
            format!("{} 里有 {n} 个控制端口，不猜哪一个。", dir.display()),
            format!(
                "{n} control sockets in {}; refusing to choose",
                dir.display()
            ),
        )),
    }
}

/// Keep a field to one line with no separators: it is written into a
/// tab-separated request.
fn clean(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c == '\t' || c == '\n' || c == '\r' {
                ' '
            } else {
                c
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// Send one command and read the answer.
///
/// # Errors
/// No socket, a connection or read failure, or an answer that is not JSON.
pub fn ask(dir: &Path, command: &str, origin: &str, reason: &str) -> Result<Value, Said> {
    let path = socket_in(dir)?;
    let mut stream =
        UnixStream::connect(&path).map_err(|e| Said::same(format!("{}: {e}", path.display())))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(|e| Said::same(e.to_string()))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| Said::same(e.to_string()))?;
    writeln!(stream, "{command}\t{}\t{}", clean(origin), clean(reason))
        .map_err(|e| Said::same(format!("write: {e}")))?;
    let mut line = String::new();
    BufReader::new(&stream)
        .take_line(&mut line)
        .map_err(|e| Said::same(format!("read: {e}")))?;
    serde_json::from_str(&line).map_err(|e| {
        Said::new(
            format!("控制端口回了一段读不懂的内容：{e}"),
            format!("the port answered something unreadable: {e}"),
        )
    })
}

trait TakeLine {
    fn take_line(&mut self, line: &mut String) -> std::io::Result<usize>;
}

impl<R: BufRead> TakeLine for R {
    /// One line, at most 1 MiB: a status is small, and a port that sends
    /// without end is not one to keep reading.
    fn take_line(&mut self, line: &mut String) -> std::io::Result<usize> {
        let mut limited = std::io::Read::take(self, 1 << 20);
        let mut buf = Vec::new();
        let n = std::io::BufRead::read_until(&mut BufReader::new(&mut limited), b'\n', &mut buf)?;
        line.push_str(&String::from_utf8_lossy(&buf));
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    #[test]
    fn a_command_goes_out_as_one_clean_line_and_the_answer_comes_back() {
        let dir = tempfile::tempdir().expect("dir");
        let listener = UnixListener::bind(dir.path().join("oq-live.x.sock")).expect("bind");
        let server = std::thread::spawn(move || {
            let (s, _) = listener.accept().expect("accept");
            let mut line = String::new();
            BufReader::new(&s).read_line(&mut line).expect("read");
            let mut w = &s;
            writeln!(w, r#"{{"ok":true,"state":"halted"}}"#).expect("write");
            line
        });
        let answer = ask(dir.path(), "halt", "deck:a\tb", "a\nreason").expect("asks");
        assert_eq!(answer["state"], "halted");
        assert_eq!(server.join().expect("server"), "halt\tdeck:a b\ta reason\n");
    }

    #[test]
    fn no_socket_or_two_is_an_error_not_a_guess() {
        let dir = tempfile::tempdir().expect("dir");
        assert!(socket_in(dir.path()).is_err());
        let _a = UnixListener::bind(dir.path().join("a.sock")).expect("a");
        let _b = UnixListener::bind(dir.path().join("b.sock")).expect("b");
        assert!(
            socket_in(dir.path())
                .unwrap_err()
                .en
                .contains("refusing to choose")
        );
    }
}
