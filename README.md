# ScalaTest Result Watcher

Quick/hacky tool to see test results live.
Useful when developing on a complex ScalaTest suite that has a lot of output without result aggregation,
when a test failure may be early on.
Running `live-server` on the default HTML reports doesn't work as well since the `index.html` is only updated at the end of the run.

## Usage

Specify your directory in `config.toml`.

Then, with [Cargo](https://www.rust-lang.org/tools/install):

```
$ cargo run --release
Starting directory watcher for: test-reports
Server running at http://localhost:3000
```
