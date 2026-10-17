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
}
