package org.ske.javaadapter;

import com.fasterxml.jackson.databind.*;
import java.io.*;
import java.nio.file.*;
import java.util.*;
import ske.adapter.v1.Adapter;

/** The adapter never invokes a compiler, class loader, or repository build. */
public final class Main {
    public static final String VERSION = "ske-adapter-java 0.6.0 (SootUp 2.0.0; bytecode 17)";
    static final int MAX_FRAME = 16 * 1024 * 1024;

    public record Profile(int profile_version, String language, List<String> class_path,
                          List<String> source_roots, int target_release) {
        static Profile parse(String text) throws IOException {
            Profile p;
            try {
                p = text.isBlank()
                    ? new Profile(1, "java", List.of("target/classes"), List.of("src/main/java"), 17)
                    : new ObjectMapper().enable(DeserializationFeature.FAIL_ON_TRAILING_TOKENS)
                        .readValue(text, Profile.class);
            } catch (IOException e) {
                throw new IOException("BUILD_PROFILE_INVALID: " + e.getMessage(), e);
            }
            if (p.profile_version != 1 || !"java".equals(p.language) || p.target_release != 17
                || p.class_path == null || p.class_path.isEmpty() || p.source_roots == null)
                throw new IOException("BUILD_PROFILE_INVALID: expected Java profile v1, target_release 17");
            return p;
        }
    }

    static Path inside(Path root, String relative) throws IOException {
        Path p = root.resolve(relative).normalize().toRealPath();
        if (Path.of(relative).isAbsolute() || !p.startsWith(root))
            throw new IOException("BUILD_PROFILE_INVALID: path outside project: " + relative);
        return p;
    }

    static byte[] readFrame(InputStream input, int maximum) throws IOException {
        byte[] header = input.readNBytes(4);
        if (header.length == 0) return null;
        if (header.length != 4) throw new EOFException("truncated frame header");
        long n = Integer.toUnsignedLong(java.nio.ByteBuffer.wrap(header).getInt());
        if (n > maximum) throw new IOException("frame exceeds limit");
        byte[] body = input.readNBytes((int)n);
        if (body.length != n) throw new EOFException("truncated frame body");
        return body;
    }

    static void writeFrame(OutputStream output, Adapter.AdapterResponse response, int maximum) throws IOException {
        byte[] b = response.toByteArray();
        if (b.length > maximum) throw new IOException("response exceeds limit");
        new DataOutputStream(output).writeInt(b.length);
        output.write(b);
        output.flush();
    }

    public static void main(String[] args) {
        if (Arrays.equals(args, new String[]{"--version"})) {
            System.out.println(VERSION);
            return;
        }
        try {
            if (args.length != 0) throw new IOException("unknown arguments");
            if (!System.getProperty("java.version").equals("24.0.1"))
                throw new IOException("TOOLCHAIN_MISMATCH: expected JVM 24.0.1");
            byte[] data = readFrame(System.in, MAX_FRAME);
            if (data == null) throw new IOException("missing request");
            var request = Adapter.AdapterRequest.parseFrom(data);
            if (request.getProtocolMajor() != 1) throw new IOException("PROTOCOL_MAJOR_UNSUPPORTED");
            boolean list = request.hasListUnits();
            if (!list && !request.hasExtractUnit()) throw new IOException("missing request body");
            String rootText = list ? request.getListUnits().getProjectRoot() : request.getExtractUnit().getProjectRoot();
            String json = list ? request.getListUnits().getBuildProfileJson() : request.getExtractUnit().getBuildProfileJson();
            var limits = list ? request.getListUnits().getLimits() : request.getExtractUnit().getLimits();
            int maximum = limits.getMaxMessageBytes() == 0 ? MAX_FRAME
                : (int)Math.min(MAX_FRAME, limits.getMaxMessageBytes());
            Profile profile = Profile.parse(json);
            Path root = Path.of(rootText).toRealPath();
            Extractor extractor = new Extractor(root, profile);
            if (list) {
                writeFrame(System.out, Adapter.AdapterResponse.newBuilder().setUnit(
                    Adapter.UnitDescriptor.newBuilder().setUnitKey("java:main").setLanguage("java")
                        .addAllSourcePaths(extractor.sourcePaths())).build(), maximum);
                writeFrame(System.out, Adapter.AdapterResponse.newBuilder().setListDone(
                    Adapter.ListUnitsDone.getDefaultInstance()).build(), maximum);
            } else {
                if (!request.getExtractUnit().getUnitKey().equals("java:main"))
                    throw new IOException("UNIT_NOT_FOUND");
                var records = extractor.extract();
                if (limits.getMaxMessages() != 0 && records.size() > limits.getMaxMessages())
                    throw new IOException("message count exceeds limit");
                for (var record : records)
                    writeFrame(System.out, Adapter.AdapterResponse.newBuilder().setRecord(record).build(), maximum);
            }
        } catch (Exception e) {
            String message = e.getMessage() == null ? e.toString() : e.getMessage();
            String code = message.startsWith("BUILD_PROFILE_INVALID") ? "BUILD_PROFILE_INVALID"
                : message.startsWith("TOOLCHAIN_MISMATCH") ? "TOOLCHAIN_MISMATCH" : "EXTRACTION_FAILED";
            try {
                writeFrame(System.out, Adapter.AdapterResponse.newBuilder().setError(
                    Adapter.AdapterError.newBuilder().setCode(code).setMessage(message)).build(), MAX_FRAME);
            } catch (IOException ignored) { /* peer closed */ }
            System.err.println("{\"event\":\"adapter_error\",\"code\":\"" + code + "\"}");
            System.exit(1);
        }
    }
}
