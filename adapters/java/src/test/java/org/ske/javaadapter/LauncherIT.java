package org.ske.javaadapter;

import static org.junit.jupiter.api.Assertions.*;
import java.io.*;
import java.nio.file.*;
import java.util.*;
import org.junit.jupiter.api.Test;
import ske.adapter.v1.Adapter;
import ske.cir.v1.Cir;

class LauncherIT {
    Path lab() throws IOException { return Path.of("../../examples/java-branch-lab").toRealPath(); }
    Process start(String... arguments) throws IOException {
        var command = new ArrayList<>(List.of(Path.of("ske-adapter-java").toAbsolutePath().toString()));
        command.addAll(List.of(arguments));
        return new ProcessBuilder(command).start();
    }
    List<Adapter.AdapterResponse> exchange(Adapter.AdapterRequest request) throws Exception {
        var process = start();
        byte[] bytes = request.toByteArray();
        try (var input = new DataOutputStream(process.getOutputStream())) {
            input.writeInt(bytes.length);
            input.write(bytes);
        }
        var responses = new ArrayList<Adapter.AdapterResponse>();
        byte[] frame;
        while ((frame = Main.readFrame(process.getInputStream(), Main.MAX_FRAME)) != null)
            responses.add(Adapter.AdapterResponse.parseFrom(frame));
        String error = new String(process.getErrorStream().readAllBytes());
        assertEquals(0, process.waitFor(), error + responses);
        assertEquals("", error, "only JSON adapter events allowed on stderr");
        return responses;
    }
    @Test void actual_framed_protocol() throws Exception {
        var units = exchange(Adapter.AdapterRequest.newBuilder().setProtocolMajor(1)
            .setListUnits(Adapter.ListUnitsRequest.newBuilder().setProjectRoot(lab().toString())).build());
        assertEquals(2, units.size());
        assertEquals("java:main", units.get(0).getUnit().getUnitKey());
        assertEquals(3, units.get(0).getUnit().getSourcePathsCount());
        assertTrue(units.get(1).hasListDone());
        var extracted = exchange(Adapter.AdapterRequest.newBuilder().setProtocolMajor(1)
            .setExtractUnit(Adapter.ExtractUnitRequest.newBuilder().setProjectRoot(lab().toString()).setUnitKey("java:main")).build());
        assertTrue(extracted.get(0).getRecord().hasHeader());
        assertTrue(extracted.get(extracted.size() - 1).getRecord().hasEndUnit());
        try (var out = new DataOutputStream(Files.newOutputStream(Path.of("target/java-ipc.cir.pb")))) {
            for (var response : extracted) {
                assertTrue(response.hasRecord(), response.toString());
                byte[] bytes = response.getRecord().toByteArray();
                out.writeInt(bytes.length); out.write(bytes);
            }
        }
    }
    @Test void jar_tampering_refused_before_jvm() throws Exception {
        Path directory = Files.createTempDirectory(Path.of("target"), "tamper-");
        Files.createDirectories(directory.resolve("target"));
        Files.writeString(directory.resolve("target/ske-adapter-java-0.6.0.jar"), "not the consented jar");
        Files.copy(Path.of("ske-adapter-java"), directory.resolve("ske-adapter-java"));
        var process = new ProcessBuilder("sh", directory.resolve("ske-adapter-java").toString(), "--version").start();
        assertEquals("", new String(process.getInputStream().readAllBytes()));
        assertTrue(new String(process.getErrorStream().readAllBytes()).contains("ADAPTER_BINARY_MISMATCH"));
        assertEquals(1, process.waitFor());
    }
    @Test void version_and_bad_frame() throws Exception {
        var version = start("--version");
        assertEquals(Main.VERSION + "\n", new String(version.getInputStream().readAllBytes()));
        assertEquals(0, version.waitFor());
        var bad = start();
        bad.getOutputStream().write(new byte[]{0, 0, 0});
        bad.getOutputStream().close();
        assertTrue(Adapter.AdapterResponse.parseFrom(Main.readFrame(bad.getInputStream(), Main.MAX_FRAME)).hasError());
        assertNull(Main.readFrame(bad.getInputStream(), Main.MAX_FRAME));
        assertEquals(1, bad.waitFor());
    }
    // Reads the effective maximum heap (bytes) that the launcher passes to the JVM.
    long launcherMaxHeap(String xmx) throws Exception {
        var command = List.of(Path.of("ske-adapter-java").toAbsolutePath().toString(), "--max-heap");
        var pb = new ProcessBuilder(command);
        if (xmx != null) pb.environment().put("SKE_JAVA_XMX", xmx);
        var process = pb.start();
        assertEquals(0, process.waitFor(), new String(process.getErrorStream().readAllBytes()));
        return Long.parseLong(new String(process.getInputStream().readAllBytes()).trim());
    }
    @Test void launcher_heap_is_configurable() throws Exception {
        // Limitation: full real repos (>~100 classes, e.g. commons-lang/commons-collections
        // full sources) overflowed the launcher's fixed -Xmx512m JVM heap and the OOM surfaced
        // as E_ADAPTER_LIMIT. The heap must be configurable via SKE_JAVA_XMX, with a sane
        // larger default, and the effective max heap honored by the spawned JVM. The adapter
        // reports its configured maximum heap through the `--max-heap` diagnostic.
        long gib = 1024L * 1024 * 1024;
        // The JVM reports a slightly rounded max-heap for a given -Xmx (up to a few % smaller,
        // growing with the request). Use a relative tolerance: the configured value must be
        // honored within ~10%, and the values must be distinguishable from the 512m cap.
        long defaultMb = launcherMaxHeap(null) / (1024 * 1024);
        // The default must be a sane larger heap for full repos (not the old 512m cap).
        assertTrue(defaultMb >= 1536, "default heap too small: " + defaultMb + "m");
        long oneG = launcherMaxHeap("1g");
        assertTrue(oneG >= 0.9 * gib && oneG < 2 * gib, "SKE_JAVA_XMX=1g not honored: " + oneG);
        long threeG = launcherMaxHeap("3g");
        assertTrue(threeG >= 2.7 * gib && threeG < 4 * gib, "SKE_JAVA_XMX=3g not honored: " + threeG);
        assertTrue(threeG > launcherMaxHeap(null), "larger configured heap must exceed the default");
        // A malformed override must be rejected loudly (never silently fall back or reach the JVM).
        var bad = new ProcessBuilder(Path.of("ske-adapter-java").toAbsolutePath().toString(), "--max-heap");
        bad.environment().put("SKE_JAVA_XMX", "banana");
        var badProcess = bad.start();
        String badErr = new String(badProcess.getErrorStream().readAllBytes());
        assertEquals(1, badProcess.waitFor());
        assertTrue(badErr.contains("LAUNCHER_CONFIG_INVALID"), badErr);
        assertEquals("", new String(badProcess.getInputStream().readAllBytes()));
    }
}
