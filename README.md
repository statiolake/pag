# pag

Simple pager which works on Windows. Written in Rust.

## Build

First you need to install [Rust](https://www.rust-lang.org/). You can install Rust by following instructions on <https://www.rust-lang.org/learn/get-started>.

After that, clone git repository and you can run:

```console
> git clone https://github.com/statiolake/pag
> cargo build
> some_command | cargo run
```

If you want to use this globally, you can install the binary. In the cloned git repository, do `cargo install --path .`. If you have installed it before, then you may need to add `--force` to override the old binary.

Make sure you have `%USERPROFILE%\.cargo\bin` or `~/.cargo/bin` in your `%PATH%` or `$PATH`. You can use this by calling `pag` anywhere.

```console
> some_command | pag
```

## Usage

Just pipe output to pag. This pager only accepts input from stdin.

```console
> type foo.txt | pag
```

In this pager, you can use these commands in normal mode (when prompt is `:`):

- `<Down>`, `j`, `Enter`: Scroll one line down
- `<Up>`, `k`: Scroll one line up
- `f`, `d`, `<Space>`: Scroll half page down
- `b`, `u`: Scroll half page up
- `g`: Go to the beginning of the output
- `G`: Go to the ending of the output
- `n`: Go to the next occurrence of current search query
- `N`: Go to the previous occurrence of current search query
- `/`: Start search

Afer typing `/`, you enter search query setting mode (prompt becomes `/`).

- `q`: cancel search query setting mode, restoring original search query
- `<Enter>`: update search query by the current input
- (other keys): input character

Although matching string will be highlighted incrementally, the screen doesn't scroll until `n` or `N` is pressed in normal mode.

## Limitations

- On legacy Windows console, vertical resize cannot be not detected (see <https://github.com/crossterm-rs/crossterm/issues/466>).
- Only UTF-8 input is supported. If you need to handle non-UTF-8 output such as cp932, use tools like iconv, [nkf](https://ja.osdn.net/projects/nkf/), etc to convert the input to UTF-8:

    ```console
    > help | nkf -w | pag
    ```

- Colored output is not supported.
