package org.ske.javaadapter;

import static org.junit.jupiter.api.Assertions.*;
import com.google.protobuf.Message;
import java.io.*;
import java.nio.*;
import java.nio.file.*;
import java.util.*;
import javax.tools.ToolProvider;
import org.junit.jupiter.api.*;
import ske.adapter.v1.Adapter;
import ske.cir.v1.Cir;

class AdapterTest {
    static Path lab;
    static List<Cir.Record> records;

    @BeforeAll static void extractTrustedFixtures() throws Exception {
        lab = Path.of("../../examples/java-branch-lab").toRealPath();
        Path classes = lab.resolve("target/classes");
        Files.createDirectories(classes);
        var arguments = new ArrayList<>(List.of("--release", "17", "-g", "-d", classes.toString()));
        try (var sources = Files.list(lab.resolve("src/main/java/example"))) {
            sources.filter(p -> p.toString().endsWith(".java")).sorted().forEach(p -> arguments.add(p.toString()));
        }
        assertEquals(0, ToolProvider.getSystemJavaCompiler().run(null, null, null, arguments.toArray(String[]::new)));
        records = new Extractor(lab, Main.Profile.parse("")).extract();
        try (var out = new DataOutputStream(Files.newOutputStream(Path.of("target/java-lab.cir.pb")))) {
            for (var record : records) { byte[] bytes = record.toByteArray(); out.writeInt(bytes.length); out.write(bytes); }
        }
    }

    static List<Cir.SourceDecisionDef> decisions(String function) {
        return records.stream().filter(Cir.Record::hasSourceDecisionDef).map(Cir.Record::getSourceDecisionDef)
            .filter(d -> d.getFunctionId().contains(function)).toList();
    }

    static String text(Cir.Span span) throws IOException {
        String[] lines = Files.readString(lab.resolve(span.getPath())).split("\n", -1);
        assertEquals(span.getStartLine(), span.getEndLine());
        byte[] line = lines[span.getStartLine() - 1].getBytes(java.nio.charset.StandardCharsets.UTF_8);
        return new String(Arrays.copyOfRange(line, span.getStartColumn() - 1, span.getEndColumn() - 1),
            java.nio.charset.StandardCharsets.UTF_8);
    }

    @Test void java_golden_decisions() throws Exception {
        var decide = decisions("int decide(int)");
        assertEquals(List.of("IF", "IF"), decide.stream().map(Cir.SourceDecisionDef::getDecisionKind).toList());
        assertEquals("x < 0", text(decide.get(0).getSpan()));
        assertEquals("x < -10", text(decide.get(1).getSpan()));
        for (var d : decide) assertEquals(List.of("TRUE", "FALSE"),
            d.getOutcomesList().stream().map(Cir.SourceOutcomeDef::getLabel).toList());
        assertTrue(decide.stream().flatMap(d -> d.getOutcomesList().stream())
            .allMatch(Cir.SourceOutcomeDef::getActivationMappingComplete));
        var day = decisions(" day(int)").get(0);
        assertEquals("SWITCH", day.getDecisionKind());
        assertEquals("n", text(day.getSpan()));
        assertEquals(List.of("CASE:0", "CASE:1", "DEFAULT"),
            day.getOutcomesList().stream().map(Cir.SourceOutcomeDef::getLabel).toList());
        var guarded = decisions("Decide: int guarded").get(0);
        assertEquals("TRY_CATCH", guarded.getDecisionKind());
        assertEquals("try", text(guarded.getSpan()));
        assertEquals(List.of("SUCCESS", "EXCEPTION"),
            guarded.getOutcomesList().stream().map(Cir.SourceOutcomeDef::getLabel).toList());
        var end = records.get(records.size() - 1).getEndUnit();
        assertEquals(5, end.getEmittedSourceDecisions());
        assertEquals(end.getExpectedSourceDecisions(), end.getEmittedSourceDecisions());
    }

    @Test void lang_types() {
        var type = records.stream().filter(Cir.Record::hasTypeDef).map(Cir.Record::getTypeDef)
            .filter(t -> t.getId().equals("java:int")).findFirst().orElseThrow();
        assertEquals("BITVECTOR", type.getKind());
        assertEquals(32, type.getBitWidth());
        assertTrue(type.getIsSigned());
    }

    @Test void java_switch_ir() {
        var sw = records.stream().filter(Cir.Record::hasInstructionDef).map(Cir.Record::getInstructionDef)
            .filter(i -> i.getOpcode().equals("SWITCH")).findFirst().orElseThrow();
        var edges = records.stream().filter(Cir.Record::hasControlEdgeDef).map(Cir.Record::getControlEdgeDef)
            .filter(e -> e.getTerminatorInstructionId().equals(sw.getId())).toList();
        assertEquals(List.of("CASE:0", "CASE:1", "DEFAULT"), edges.stream().map(Cir.ControlEdgeDef::getOutcomeLabel).toList());
        var constants = new HashMap<String, Integer>();
        records.stream().filter(Cir.Record::hasValueDef).map(Cir.Record::getValueDef)
            .filter(v -> v.hasLiteral() && v.getLiteral().hasMachineBitsBe() && v.getLiteral().getMachineBitsBe().size() == 4)
            .forEach(v -> constants.put(v.getId(), ByteBuffer.wrap(v.getLiteral().getMachineBitsBe().toByteArray()).getInt()));
        assertEquals(List.of(1), edges.get(0).getCaseValueIdsList().stream().map(constants::get).toList());
        assertEquals(List.of(2, 3), edges.get(1).getCaseValueIdsList().stream().map(constants::get).toList());
    }

    @Test void java_invoke_exception() {
        var invoke = records.stream().filter(Cir.Record::hasInstructionDef).map(Cir.Record::getInstructionDef)
            .filter(i -> i.getId().contains("Decide: int guarded") && i.getOpcode().equals("INVOKE")).findFirst().orElseThrow();
        var edges = records.stream().filter(Cir.Record::hasControlEdgeDef).map(Cir.Record::getControlEdgeDef)
            .filter(e -> e.getTerminatorInstructionId().equals(invoke.getId())).toList();
        assertEquals("NORMAL", edges.get(0).getEdgeKind());
        assertEquals("java:java.lang.RuntimeException", edges.get(1).getExceptionFilterTypeId());
        assertTrue(edges.get(edges.size() - 1).getExceptionCatchAll());
    }

    @Test void java_reflection_unknown() {
        var invoke = records.stream().filter(Cir.Record::hasInstructionDef).map(Cir.Record::getInstructionDef)
            .filter(i -> i.getId().contains("ReflectionUnknown: java.lang.Class load") && i.getOpcode().equals("INVOKE")).findFirst().orElseThrow();
        assertEquals("REFLECTIVE", invoke.getPayload().getCall().getDispatchKind());
        assertFalse(invoke.getPayload().getCall().getWorldClosed());
        assertTrue(records.stream().filter(Cir.Record::hasCallTargetDef).noneMatch(r ->
            r.getCallTargetDef().getCallInstructionId().equals(invoke.getId())));
        assertTrue(records.stream().filter(Cir.Record::hasDiagnostic).anyMatch(r ->
            r.getDiagnostic().getAffectedFunctionId().contains("ReflectionUnknown: java.lang.Class load")
                && r.getDiagnostic().getCode().equals("CALL_TARGET_UNRESOLVED")));
    }

    @Test void java_multi_catch_finally() {
        var invocations = records.stream().filter(Cir.Record::hasInstructionDef).map(Cir.Record::getInstructionDef)
            .filter(i -> i.getId().contains("MultiCatchFinally: int guarded") && i.getOpcode().equals("INVOKE")).toList();
        assertTrue(invocations.size() >= 4, "parseInt and all duplicated finally println calls");
        var edges = records.stream().filter(Cir.Record::hasControlEdgeDef).map(Cir.Record::getControlEdgeDef)
            .filter(e -> e.getTerminatorInstructionId().equals(invocations.get(0).getId())).toList();
        assertEquals(List.of("java:java.lang.NumberFormatException", "java:java.lang.RuntimeException", ""),
            edges.stream().filter(e -> e.getEdgeKind().equals("EXCEPTION")).map(Cir.ControlEdgeDef::getExceptionFilterTypeId).toList());
        assertTrue(records.stream().filter(Cir.Record::hasDiagnostic).anyMatch(r ->
            r.getDiagnostic().getAffectedFunctionId().contains("MultiCatchFinally: int guarded")
                && r.getDiagnostic().getCode().equals("EXCEPTION_FLOW_PARTIAL")
                && r.getDiagnostic().getImpact().equals("INVENTORY")));
        assertEquals("PARTIAL", records.get(records.size() - 1).getEndUnit().getInventoryStatus());
    }

    @Test void framing_and_profile() throws Exception {
        assertArrayEquals(new byte[0], Main.readFrame(new ByteArrayInputStream(new byte[4]), 16));
        assertNull(Main.readFrame(new ByteArrayInputStream(new byte[0]), 16));
        assertThrows(EOFException.class, () -> Main.readFrame(new ByteArrayInputStream(new byte[]{0}), 16));
        assertThrows(EOFException.class, () -> Main.readFrame(new ByteArrayInputStream(new byte[]{0, 0, 0, 1}), 16));
        assertThrows(IOException.class, () -> Main.readFrame(new ByteArrayInputStream(new byte[]{-1, -1, -1, -1}), 16));
        assertThrows(IOException.class, () -> Main.Profile.parse("{\"unknown\":1}"));
        assertThrows(IOException.class, () -> Main.Profile.parse("{} {}"));
        assertThrows(IOException.class, () -> Main.inside(lab, "../"));
        var response = Adapter.AdapterResponse.newBuilder().setListDone(Adapter.ListUnitsDone.getDefaultInstance()).build();
        var out = new ByteArrayOutputStream();
        Main.writeFrame(out, response, 1024);
        assertEquals(response, Adapter.AdapterResponse.parseFrom(Main.readFrame(new ByteArrayInputStream(out.toByteArray()), 1024)));
    }

    @Test void deterministic_extraction() throws Exception {
        assertEquals(records, new Extractor(lab, Main.Profile.parse("")).extract());
    }

    @Test void local_type_contracts() {
        var values = new HashMap<String, Cir.ValueDef>();
        var types = new HashMap<String, Cir.TypeDef>();
        for (var record : records) {
            if (record.hasValueDef()) values.put(record.getValueDef().getId(), record.getValueDef());
            if (record.hasTypeDef()) types.put(record.getTypeDef().getId(), record.getTypeDef());
        }
        for (var record : records) if (record.hasInstructionDef()) {
            var i = record.getInstructionDef();
            if (i.getOpcode().equals("COPY")) assertEquals(values.get(i.getResultValueId()).getTypeId(),
                values.get(i.getOperandValueIds(0)).getTypeId(), i.toString());
            if (i.getOpcode().equals("INVOKE")) {
                var callable = types.get(values.get(i.getOperandValueIds(0)).getTypeId());
                assertEquals(callable.getParamTypeIdsList(), i.getOperandValueIdsList().subList(1, i.getOperandValueIdsCount())
                    .stream().map(v -> values.get(v).getTypeId()).toList(), i.toString());
                assertEquals(callable.getResultTypeIdsList(), i.getResultValueId().isEmpty() ? List.of()
                    : List.of(values.get(i.getResultValueId()).getTypeId()), i.toString());
            }
        }
    }

    @Test void general_extraction_not_fixture_specific() throws Exception {
        Path project = Files.createTempDirectory(Path.of("target"), "general-").toRealPath();
        Files.createDirectories(project.resolve("src/main/java"));
        Files.createDirectories(project.resolve("target/classes"));
        Path java = project.resolve("src/main/java/Different.java");
        Files.writeString(java, """
            public class Different {
                public static int changed(int amount) {
                    if (amount <= 7) return 9;
                    return 2;
                }
                public static int caller(int x) { return changed(x); }
            }
            """);
        assertEquals(0, ToolProvider.getSystemJavaCompiler().run(null, null, null, "--release", "17", "-g",
            "-d", project.resolve("target/classes").toString(), java.toString()));
        var extracted = new Extractor(project, Main.Profile.parse("")).extract();
        var decision = extracted.stream().filter(Cir.Record::hasSourceDecisionDef)
            .map(Cir.Record::getSourceDecisionDef).findFirst().orElseThrow();
        assertTrue(decision.getFunctionId().contains("Different: int changed"));
        assertTrue(decision.getOutcomesList().stream().allMatch(Cir.SourceOutcomeDef::getActivationMappingComplete));
        assertTrue(extracted.stream().filter(Cir.Record::hasCallTargetDef).anyMatch(r ->
            r.getCallTargetDef().getTargetFunctionId().equals(decision.getFunctionId())));
        assertTrue(extracted.stream().filter(Cir.Record::hasValueDef).map(Cir.Record::getValueDef)
            .anyMatch(v -> v.hasLiteral() && v.getLiteral().hasMachineBitsBe()
                && v.getLiteral().getMachineBitsBe().equals(com.google.protobuf.ByteString.copyFrom(new byte[]{0, 0, 0, 7}))));
        // The parser/IR operand cross-check must refuse an attestation for stale source.
        Files.writeString(java, Files.readString(java).replace("amount <= 7", "amount <= 8"));
        var stale = new Extractor(project, Main.Profile.parse("")).extract();
        assertFalse(stale.stream().filter(Cir.Record::hasSourceDecisionDef)
            .flatMap(r -> r.getSourceDecisionDef().getOutcomesList().stream())
            .anyMatch(Cir.SourceOutcomeDef::getActivationMappingComplete));
    }
}
