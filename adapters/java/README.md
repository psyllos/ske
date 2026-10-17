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
