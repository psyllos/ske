package org.ske.javaadapter;

import static org.junit.jupiter.api.Assertions.*;
import com.google.protobuf.Message;
import java.io.*;
import java.nio.*;
import java.nio.file.*;
import java.util.*;
import java.util.stream.*;
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

    @Test void array_store_and_float_arithmetic_are_valid_or_opaque() throws Exception {
        // Regression for two real-project failures:
        //  - an array store `$a[i] = v` (a non-Local lhs) previously allocated a pre-defined
        //    SSA slot that nothing ever defined -> E_SSA_DOMINANCE (undefined vN) in the core ;
        //  - IEEE_FLOAT arithmetic (ADD/MUL over double) was emitted as-is ->
        //    E_TYPE_MISMATCH, but the core only models BITVECTOR arithmetic.
        Path project = Files.createTempDirectory(Path.of("target"), "ops-").toRealPath();
        Files.createDirectories(project.resolve("src/main/java"));
        Files.createDirectories(project.resolve("target/classes"));
        Path java = project.resolve("src/main/java/Ops.java");
        Files.writeString(java, """
            public class Ops {
                public static double avg(double a, double b) {
                    return (a + b) / 2.0;
                }
                public static String fmt(int count, String label) {
                    Object[] args = new Object[] { count, label, Boolean.TRUE };
                    return String.format("%d %s %s", args);
                }
                public static int boundary(int n) {
                    int[] t = new int[2];
                    t[0] = n;
                    t[1] = t[0] + 1;
                    return t[1];
                }
            }
            """);
        assertEquals(0, ToolProvider.getSystemJavaCompiler().run(null, null, null, "--release", "17", "-g",
            "-d", project.resolve("target/classes").toString(), java.toString()));
        var extracted = new Extractor(project, Main.Profile.parse("")).extract();
        // Every SSA value must be defined exactly once by a real instruction (no orphan).
        var values = extracted.stream().filter(Cir.Record::hasValueDef).map(Cir.Record::getValueDef)
            .filter(v -> v.getRole().equals("SSA")).map(Cir.ValueDef::getId).collect(Collectors.toSet());
        var defined = extracted.stream().filter(Cir.Record::hasInstructionDef)
            .map(Cir.Record::getInstructionDef).filter(i -> !i.getResultValueId().isEmpty())
            .map(Cir.InstructionDef::getResultValueId).collect(Collectors.toSet());
        values.removeAll(defined);
        assertTrue(values.isEmpty(), "orphaned SSA values: " + values);
        // The double arithmetic in avg() must be opaque (no BITVECTOR ADD/MUL over IEEE_FLOAT).
        var arithmetic = extracted.stream().filter(Cir.Record::hasInstructionDef).map(Cir.Record::getInstructionDef)
            .filter(i -> Set.of("ADD", "SUB", "MUL", "DIV", "REM").contains(i.getOpcode()))
            .filter(i -> i.getBlockId().contains("avg")).toList();
        assertTrue(arithmetic.isEmpty(), "float arithmetic leaked as bitvector op: " + arithmetic);
        // ... and the float computation surfaces as an opaque operation with a diagnostic.
        assertTrue(extracted.stream().filter(Cir.Record::hasInstructionDef)
            .map(Cir.Record::getInstructionDef)
            .anyMatch(i -> i.getBlockId().contains("avg") && i.getOpcode().equals("OPAQUE_OP")));
        assertTrue(extracted.stream().filter(Cir.Record::hasDiagnostic).anyMatch(r ->
            r.getDiagnostic().getAffectedFunctionId().contains("Ops: double avg")
                && r.getDiagnostic().getCode().equals("UNSUPPORTED_INSTRUCTION")));
    }

    @Test void boolean_guards_and_char_switch_are_modeled() throws Exception {
        // Regression for real-project failures (godotenv-style null guards, boolean
        // invoke results, char switches) that previously ABORTED the whole unit:
        //  - `if (s == null)` produced a BRANCH guard typed java:int (BITVECTOR) instead
        //    of BOOL -> E_TYPE_MISMATCH.
        //  - a boolean-returning call (s.equals) bound an int-typed local to a boolean
        //    result -> E_TYPE_MISMATCH at INVOKE.
        //  - a char switch emitted an int tag with char/byte case constants -> E_TYPE_MISMATCH.
        Path project = Files.createTempDirectory(Path.of("target"), "bool-").toRealPath();
        Files.createDirectories(project.resolve("src/main/java"));
        Files.createDirectories(project.resolve("target/classes"));
        Path java = project.resolve("src/main/java/Bool.java");
        Files.writeString(java, """
            public class Bool {
                public static class T {}
                public static T make() { return null; }
                public static String nullGuard(T t) {
                    if (t == null) return "a";
                    return "b";
                }
                public static boolean same(String a, String b) { return a.equals(b); }
                public static String kind(char c) {
                    switch (c) {
                        case 34: return "quote";
                        case 92: return "slash";
                        case 10: return "nl";
                        default: return "other";
                    }
                }
            }
            """);
        assertEquals(0, ToolProvider.getSystemJavaCompiler().run(null, null, null, "--release", "17", "-g",
            "-d", project.resolve("target/classes").toString(), java.toString()));
        var extracted = new Extractor(project, Main.Profile.parse("")).extract();
        // Every BRANCH guard must be BOOL-typed (the null guard would previously be int).
        var values = new HashMap<String, Cir.ValueDef>();
        var types = new HashMap<String, Cir.TypeDef>();
        for (var record : extracted) {
            if (record.hasValueDef()) values.put(record.getValueDef().getId(), record.getValueDef());
            if (record.hasTypeDef()) types.put(record.getTypeDef().getId(), record.getTypeDef());
        }
        for (var record : extracted) if (record.hasInstructionDef()) {
            var i = record.getInstructionDef();
            if (i.getOpcode().equals("BRANCH")) {
                var guard = values.get(i.getOperandValueIds(0));
                assertEquals("BOOL", types.get(guard.getTypeId()).getKind(), "BRANCH guard not BOOL: " + guard);
            }
        }
        // There must be a real SWITCH (kind(char)) and a boolean-returning INVOKE (same).
        assertTrue(extracted.stream().filter(Cir.Record::hasInstructionDef).map(Cir.Record::getInstructionDef)
            .anyMatch(i -> i.getOpcode().equals("SWITCH")));
        assertTrue(extracted.stream().filter(Cir.Record::hasInstructionDef).map(Cir.Record::getInstructionDef)
            .anyMatch(i -> i.getOpcode().equals("INVOKE") && i.getBlockId().contains("same")));
        // A void (zero-result) check: extraction must have ended with EndUnit inventory COMPLETE
        // or PARTIAL, never FAILED.
        var end = extracted.get(extracted.size() - 1).getEndUnit();
        assertTrue(end.getInventoryStatus().equals("COMPLETE") || end.getInventoryStatus().equals("PARTIAL"),
            "unit inventory " + end.getInventoryStatus());
    }

    @Test void subtype_to_supertype_invoke_is_modeled() throws Exception {
        // Regression for the java-a real-project finding (effective-java item30
        // RecursiveTypeBound): a non-void INVOKE whose declared return type is more
        // specific than the JVM-typed local slot (java.util.List vs java.lang.Object)
        // aborted the whole unit with E_TYPE_MISMATCH at the INVOKE, because the CIR
        // validator requires the INVOKE result value to equal the callee's declared
        // return type. Pass-through subtype->supertype parameters must also extract
        // (Object[] <- String[], Collection <- List).
        Path project = Files.createTempDirectory(Path.of("target"), "subtype-").toRealPath();
        Files.createDirectories(project.resolve("src/main/java"));
        Files.createDirectories(project.resolve("target/classes"));
        Path java = project.resolve("src/main/java/Sub.java");
        Files.writeString(java, """
            import java.util.*;
            public class Sub {
                public static <E extends Comparable<E>> E max(Collection<E> c) {
                    return c.iterator().next();
                }
                public static void main(String[] args) {
                    List<String> argList = Arrays.asList(args);
                    System.out.println(Sub.max(argList));
                }
            }
            """);
        assertEquals(0, ToolProvider.getSystemJavaCompiler().run(null, null, null, "--release", "17", "-g",
            "-d", project.resolve("target/classes").toString(), java.toString()));
        var extracted = new Extractor(project, Main.Profile.parse("")).extract();
        // The unit must not abort: inventory COMPLETE or PARTIAL, never FAILED, and no
        // E_TYPE_MISMATCH inside a UNIT_FAILED diagnostic.
        var end = extracted.get(extracted.size() - 1).getEndUnit();
        assertTrue(end.getInventoryStatus().equals("COMPLETE") || end.getInventoryStatus().equals("PARTIAL"),
            "unit inventory " + end.getInventoryStatus());
        assertTrue(extracted.stream().filter(Cir.Record::hasDiagnostic)
            .map(Cir.Record::getDiagnostic)
            .noneMatch(d -> d.getCode().equals("UNIT_FAILED") || d.getCode().equals("E_TYPE_MISMATCH")),
            "unexpected ingestion error");
        // The Arrays.asList INVOKE result slot must be typed by the declared return
        // (java.util.List), so the result equals the callee's declared return type.
        var values = new HashMap<String, Cir.ValueDef>();
        for (var record : extracted) if (record.hasValueDef())
            values.put(record.getValueDef().getId(), record.getValueDef());
        var asListInvoke = extracted.stream().filter(Cir.Record::hasInstructionDef)
            .map(Cir.Record::getInstructionDef)
            .filter(i -> i.getOpcode().equals("INVOKE") && i.getOperandValueIdsList().get(0).contains("asList"))
            .findFirst().orElseThrow();
        assertEquals("java:java.util.List", values.get(asListInvoke.getResultValueId()).getTypeId(),
            "Arrays.asList result must be typed java.util.List");
        // And a subtype->supertype receiver side: max(Collection) with a List argument.
        assertTrue(extracted.stream().filter(Cir.Record::hasDiagnostic).map(Cir.Record::getDiagnostic)
            .noneMatch(d -> d.getCode().equals("E_TYPE_MISMATCH")));
    }
}
