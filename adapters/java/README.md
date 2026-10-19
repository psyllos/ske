# Java adapter v0.6

Build the trusted adapter, not the studied repository:

```sh
cd adapters/java
mvn -B verify
./ske-adapter-java --version
```

Requires Maven 3.10.0 and JDK 24.0.1. Maven generates the executable launcher
with the shaded jar's SHA-256 embedded. Rebuild after editing source; copying a
different jar under an old launcher fails closed. Generated artifacts are ignored.
Do not run two Maven builds concurrently in the same adapter directory.

## JVM heap (full repositories)

The launcher JVM heap is configurable through `SKE_JAVA_XMX` and defaults to `-Xmx2g`
(not the historical fixed 512 MB cap, which made full real repositories — hundreds of
classes, e.g. commons-lang/commons-collections complete sources — die with an OOM that
`crates/ske-ingest` surfaced as `E_ADAPTER_LIMIT`).

Accepted values: a plain size (`2g`, `1536m`, `1G`) or an explicit `-Xmx...` flag; a
malformed value is rejected loudly by the launcher (`LAUNCHER_CONFIG_INVALID`) before the
JVM starts. `ske ingest java` forwards `SKE_JAVA_XMX` to the launcher, so the override
is effective in the product path, not only when invoking the launcher directly. The
default is safe on CI: `-Xms32m` keeps the initial committed heap small, and the ingest
RSS watchdog budget (default 4096 MB) leaves room below it for a 2 GB heap.

The effective heap can be inspected with `./ske-adapter-java --max-heap` (prints the JVM's
configured maximum heap in bytes).

## Implicit exceptions of non-invoke statements

A try/catch whose protected range contains a non-invoke throwing statement (an array
load/store, arithmetic, cast, or field access) is now conservatively dispatched: the
affected blocks end in a `SWITCH` on an opaque dispatch tag (one `CASE` per caught
handler in original exception-table order, `DEFAULT` to the normal successor) instead of
leaving the handler blocks disconnected. Previously that emitted a dead handler chain
whose caught-exception SSA value had no dominating definition and the core rejected the
whole unit with `E_SSA_DOMINANCE` (real projects: json-simple `Yylex.zzScanError`). Each
such statement carries an `EXCEPTION_FLOW_PARTIAL` diagnostic noting the implicit
exceptions are over-approximated (`SYMBOLIC` impact); the CIR models exception edges only
on `INVOKE` terminators, so the dispatch switch is the conservative, schema-faithful form.
Invoke-based try/catch keeps its exact `INVOKE` NORMAL/EXCEPTION edges.

The adapter reads existing class directories/jars and optional Java 17 sources.
It never invokes a studied build, compiler, annotation processor, classloader,
tests or repository scripts. Maven dependency resolution builds this adapter only.
The default closed build profile is:

```json
{"profile_version":1,"language":"java","class_path":["target/classes"],"source_roots":["src/main/java"],"target_release":17}
```

Each path is relative to the studied root and must resolve inside it. All
classpath entries are inventoried; include only the intended analysis scope.
For the **trusted fixture only**, tests run the equivalent of:

```sh
cd examples/java-branch-lab
mkdir -p target/classes
javac --release 17 -g -d target/classes src/main/java/example/*.java
```

One unit `java:main`; source symbols include `example.Decide.decide`,
`example.Decide.day`, `example.Decide.guarded`, `example.ReflectionUnknown.load`,
and `example.MultiCatchFinally.guarded`. Existing binary IPC and CIR 1.3 are
unchanged. `verify` runs both actual extraction tests and packaged-launcher
framing/tampering tests. Tests emit actual (not golden) framed CIR to
`target/java-lab.cir.pb` and `target/java-ipc.cir.pb` for importer integration.

Limits and mandatory license audit: `docs/adr/ADR-022-java.md` and
`docs/worker-java-v06.md`. The development shaded jar is not approved for public
redistribution; SootUp's LGPL obligations require a distribution-specific review.
