package org.ske.javaadapter;

import com.github.javaparser.ast.Node;
import com.github.javaparser.ast.expr.*;
import com.github.javaparser.ast.stmt.*;
import com.google.protobuf.ByteString;
import com.google.protobuf.Message;
import java.io.*;
import java.nio.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.security.*;
import java.util.*;
import java.util.jar.JarFile;
import org.objectweb.asm.*;
import ske.cir.v1.Cir;
import sootup.core.jimple.basic.*;
import sootup.core.jimple.common.constant.*;
import sootup.core.jimple.common.expr.*;
import sootup.core.jimple.common.ref.*;
import sootup.core.jimple.common.stmt.*;
import sootup.core.jimple.javabytecode.stmt.JSwitchStmt;
import sootup.core.model.*;
import sootup.java.bytecode.frontend.inputlocation.JavaClassPathAnalysisInputLocation;
import sootup.java.core.*;
import sootup.java.core.views.JavaView;

/** Jimple controls all IR extraction; JavaParser supplies only source inventory/provenance. */
final class Extractor {
    final Path root;
    final Main.Profile profile;
    final Sources sources;
    final List<Path> classPath = new ArrayList<>();
    final List<Cir.Record> records = new ArrayList<>();
    final Map<String, String> valueTypes = new HashMap<>();
    final Set<String> types = new HashSet<>(), stubs = new HashSet<>();
    final Map<String, String> typeKinds = new HashMap<>();
    final Set<String> projectMethods = new HashSet<>();
    long edges, decisions;
    boolean partial;

    Extractor(Path root, Main.Profile profile) throws IOException {
        this.root = root;
        this.profile = profile;
        this.sources = new Sources(root, profile);
        for (String p : profile.class_path()) classPath.add(Main.inside(root, p));
    }

    List<String> sourcePaths() { return sources.files.stream().map(Sources.File::path).toList(); }

    static String hash(byte[] data) {
        try { return HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(data)); }
        catch (NoSuchAlgorithmException e) { throw new AssertionError(e); }
    }

    void add(Message message) {
        if (message instanceof Cir.ValueDef value) valueTypes.put(value.getId(), value.getTypeId());
        var builder = Cir.Record.newBuilder();
        var field = builder.getDescriptorForType().getFields().stream()
            .filter(f -> f.getMessageType().equals(message.getDescriptorForType())).findFirst().orElseThrow();
        records.add(builder.setField(field, message).build());
    }

    void diagnostic(String function, String code, String impact, String message) {
        partial = true;
        add(Cir.Diagnostic.newBuilder().setSeverity("WARNING").setCode(code).setImpact(impact)
            .setAffectedFunctionId(function).setMessage(message).build());
    }

    String type(String name) {
        String id = "java:" + name;
        if (types.add(id)) {
            var t = Cir.TypeDef.newBuilder().setId(id).setCanonicalDescriptor(id);
            String kind;
            switch (name) {
                case "void" -> kind = "VOID";
                case "boolean" -> kind = "BOOL";
                case "byte", "short", "int", "long", "char" -> kind = "BITVECTOR";
                case "float", "double" -> kind = "IEEE_FLOAT";
                case "java.lang.String" -> kind = "STRING";
                default -> kind = "OPAQUE";
            }
            switch (kind) {
                case "VOID" -> t.setKind("VOID");
                case "BOOL" -> t.setKind("BOOL");
                case "BITVECTOR" -> t.setKind("BITVECTOR")
                    .setBitWidth(switch (name) { case "byte" -> 8; case "short", "char" -> 16; case "int" -> 32; default -> 64; })
                    .setIsSigned(!name.equals("char"));
                case "IEEE_FLOAT" -> t.setKind("IEEE_FLOAT").setBitWidth(name.equals("float") ? 32 : 64);
                case "STRING" -> t.setKind("STRING");
                default -> t.setKind("OPAQUE");
            }
            typeKinds.put(id, kind);
            add(t.build());
        }
        return id;
    }

    boolean isBitvector(String typeId) { return "BITVECTOR".equals(typeKinds.get(typeId)); }
    boolean isFloat(String typeId) { return "IEEE_FLOAT".equals(typeKinds.get(typeId)); }

    String functionType(String id, List<String> parameters, List<String> results) {
        String tid = id + "/type";
        if (types.add(tid)) add(Cir.TypeDef.newBuilder().setId(tid).setKind("FUNCTION")
            .setCanonicalDescriptor(tid).addAllParamTypeIds(parameters).addAllResultTypeIds(results).build());
        return tid;
    }

    List<Cir.Record> extract() throws IOException {
        // Fingerprint exact profile + bytecode contents, not mtimes or machine-specific absolute paths.
        var fingerprint = new ByteArrayOutputStream();
        fingerprint.write(profile.toString().getBytes(StandardCharsets.UTF_8));
        for (Path path : classPath) {
            if (Files.isDirectory(path)) {
                try (var stream = Files.walk(path)) {
                    for (Path p : stream.filter(p -> p.toString().endsWith(".class")).sorted().toList()) {
                        Path real = p.toRealPath();
                        if (!real.startsWith(root)) throw new IOException("class symlink outside project");
                        byte[] bytes = Files.readAllBytes(real);
                        checkClass(bytes);
                        fingerprint.write(root.relativize(p).toString().getBytes(StandardCharsets.UTF_8));
                        fingerprint.write(bytes);
                    }
                }
            } else {
                fingerprint.write(Files.readAllBytes(path));
                try (var jar = new JarFile(path.toFile())) {
                    for (var entry : jar.stream().filter(e -> e.getName().endsWith(".class")).toList())
                        checkClass(jar.getInputStream(entry).readAllBytes());
                }
            }
        }
        add(Cir.Header.newBuilder().setCirMajor(1).setCirMinor(3).setAdapterName("ske-adapter-java")
            .setAdapterVersion("0.6.0").setLanguage("java").setToolchainVersion(System.getProperty("java.version"))
            .setBuildFingerprint(hash(fingerprint.toByteArray())).setSemanticProfile("java-17-jvm")
            .addAllCapabilityFlags(List.of("SOURCE_AST", "JIMPLE_CFG", "SWITCH", "INVOKE", "OPEN_WORLD")).build());
        add(Cir.UnitDef.newBuilder().setUnitId("java:main").setUnitKey("java:main").setLanguage("java").build());
        for (var source : sources.files) add(Cir.SourceFileDef.newBuilder().setPath(source.path())
            .setLanguage("java").setContentSha256(hash(source.text().getBytes(StandardCharsets.UTF_8))).build());
        var view = new JavaView(classPath.stream().map(p ->
            (sootup.core.inputlocation.AnalysisInputLocation)new JavaClassPathAnalysisInputLocation(p.toString())).toList());
        var classes = view.getClasses().sorted(Comparator.comparing(c -> c.getType().toString())).toList();
        if (classes.isEmpty()) throw new IOException("no precompiled classes found; compile trusted fixtures separately");
        for (var c : classes) for (var method : c.getMethods()) projectMethods.add(method.getSignature().toString());
        for (var c : classes) {
            for (var method : c.getMethods().stream().sorted(Comparator.comparing(m -> m.getSignature().toString())).toList())
                new Lower(method).emit();
        }
        if (sources.count() != decisions) diagnostic("", "SOURCE_MAPPING_PARTIAL", "INVENTORY",
            "Source decisions in methods without uniquely matched bytecode, initializers, or unsupported syntax");
        add(Cir.EndUnit.newBuilder().setUnitId("java:main").setInventoryStatus(partial ? "PARTIAL" : "COMPLETE")
            .setExpectedSourceDecisions(sources.count()).setEmittedSourceDecisions(decisions).setEmittedIrEdges(edges).build());
        return records;
    }

    static void checkClass(byte[] bytes) throws IOException {
        if (bytes.length < 8 || ByteBuffer.wrap(bytes).getInt() != 0xcafebabe)
            throw new IOException("invalid classfile");
        int major = Short.toUnsignedInt(ByteBuffer.wrap(bytes, 6, 2).getShort());
        if (major > 61) throw new IOException("TOOLCHAIN_MISMATCH: expected bytecode <= Java 17 (61), got " + major);
    }

    /** Original exception table order; SootUp's exceptional successor map is not an ordered contract. */
    List<String> handlerOrder(JavaSootMethod method) throws IOException {
        String owner = method.getSignature().getDeclClassType().toString().replace('.', '/') + ".class";
        byte[] bytes = null;
        for (Path path : classPath) {
            if (Files.isDirectory(path) && Files.isRegularFile(path.resolve(owner))) {
                bytes = Files.readAllBytes(path.resolve(owner)); break;
            } else if (!Files.isDirectory(path)) {
                try (var jar = new JarFile(path.toFile())) {
                    var entry = jar.getJarEntry(owner);
                    if (entry != null) { bytes = jar.getInputStream(entry).readAllBytes(); break; }
                }
            }
        }
        if (bytes == null) throw new IOException("missing original bytecode: " + owner);
        var order = new ArrayList<String>();
        new ClassReader(bytes).accept(new ClassVisitor(Opcodes.ASM9) {
            @Override public MethodVisitor visitMethod(int access, String name, String descriptor, String signature, String[] exceptions) {
                var parameters = Arrays.stream(org.objectweb.asm.Type.getArgumentTypes(descriptor))
                    .map(org.objectweb.asm.Type::getClassName).toList();
                if (!name.equals(method.getName()) || !parameters.equals(method.getParameterTypes().stream().map(Object::toString).toList()))
                    return null;
                return new MethodVisitor(Opcodes.ASM9) {
                    @Override public void visitTryCatchBlock(Label start, Label end, Label handler, String type) {
                        String key = type == null ? "java.lang.Throwable" : type.replace('/', '.');
                        if (!order.contains(key)) order.add(key);
                        else diagnostic("java:" + method.getSignature(), "EXCEPTION_FLOW_PARTIAL", "INVENTORY",
                            "Repeated exception table type across protected ranges; per-range dispatch not certified: " + key);
                    }
                };
            }
        }, ClassReader.SKIP_DEBUG | ClassReader.SKIP_FRAMES);
        return order;
    }

    final class Lower {
        final JavaSootMethod method;
        final String id;
        final Sources.Method source;
        final List<String> returnTypes = new ArrayList<>(), paramIds = new ArrayList<>();
        final Map<Value, String> values = new HashMap<>();
        final Map<Value, Integer> definitions = new HashMap<>();
        final Map<Stmt, String> blocks = new IdentityHashMap<>();
        final Map<Stmt, List<Cir.ControlEdgeDef>> statementEdges = new IdentityHashMap<>();
        final Map<String, String> constants = new HashMap<>();
        final List<String> handlers;
        // Catch-handler statements that already have an incoming edge. `invoke()` emits
        // exception edges to call handlers; implicit-exception dispatch switches connect
        // the remaining ones. Any handler left with no incoming edge becomes a dead block
        // whose pre-allocated caught-exception SSA value the core rejects (E_SSA_DOMINANCE).
        final Set<Stmt> connectedHandlers = new HashSet<>();
        Body body;
        String block;
        int instruction, sequence, blockCount;

        Lower(JavaSootMethod method) throws IOException {
            this.method = method;
            id = "java:" + method.getSignature();
            source = sources.method(method.getSignature().getDeclClassType().toString(), method.getName(),
                method.getParameterTypes().stream().map(Object::toString).toList());
            handlers = handlerOrder(method);
        }

        String fresh() { return id + "/v" + sequence++; }
        String ssa(String type) {
            String v = fresh();
            add(Cir.ValueDef.newBuilder().setId(v).setFunctionId(id).setTypeId(type).setRole("SSA").build());
            return v;
        }
        Cir.InstructionDef.Builder instruction(String opcode, boolean terminator) {
            return Cir.InstructionDef.newBuilder().setId(block + "/i" + instruction).setBlockId(block)
                .setOrdinal(instruction++).setOpcode(opcode).setIsTerminator(terminator);
        }
        String havoc(String type) {
            String v = ssa(type);
            add(instruction("HAVOC", false).setResultValueId(v).build());
            return v;
        }
        String localType(Value value) {
            // Keep boolean, byte, short and char as their declared types: narrowing to
            // int at the SSA-slot level breaks boolean invoke results (BOOL return vs int
            // local) and char/byte switches (tag vs case-constant type). JVM widening to
            // int is applied explicitly where computation needs it (widenToInt in binops,
            // coerce in call args / returns / case constants).
            return type(value.getType().toString());
        }
        String coerce(String value, String target) {
            String actual = valueTypes.get(value);
            if (target.equals(actual)) return value;
            Map<String, Integer> widths = Map.of("java:byte", 8, "java:short", 16, "java:char", 16, "java:int", 32, "java:long", 64);
            String result = ssa(target);
            if (widths.containsKey(actual) && widths.containsKey(target)) {
                int from = widths.get(actual), to = widths.get(target);
                String op = from == to ? "BITCAST" : from > to ? "TRUNCATE" : actual.equals("java:char") ? "ZERO_EXTEND" : "SIGN_EXTEND";
                add(instruction(op, false).setResultValueId(result).addOperandValueIds(value).build());
            } else {
                add(instruction("OPAQUE_OP", false).setResultValueId(result).addOperandValueIds(value)
                    .setPayload(opaque("JVM conversion " + actual + " -> " + target)).build());
                diagnostic(id, "UNSUPPORTED_INSTRUCTION", "SYMBOLIC", "Opaque JVM conversion " + actual + " -> " + target);
            }
            return result;
        }
        // JVM promotes byte/short/char to int in arithmetic/comparison; the CIR core
        // requires equal operand types, so widen narrow BITVECTOR operands to int.
        String widenToInt(String value) {
            String t = valueTypes.get(value);
            return Set.of("java:byte", "java:short", "java:char").contains(t) ? coerce(value, type("int")) : value;
        }
        Cir.OperationPayload opaque(String construct) {
            return Cir.OperationPayload.newBuilder().setOpaque(Cir.OpaqueSemantics.newBuilder()
                .setSourceConstruct(construct).setReason("Java semantics not modeled")
                .setMayMutateMemory(true).setMayExitAbnormally(true)).build();
        }
        String opaqueValue(Value value) {
            String v = ssa(type(value.getType().toString()));
            add(instruction("OPAQUE_OP", false).setResultValueId(v).setPayload(opaque(value.toString())).build());
            diagnostic(id, "UNSUPPORTED_INSTRUCTION", "EFFECTS", value.toString());
            diagnostic(id, "EXCEPTION_FLOW_PARTIAL", "INVENTORY", "Implicit exceptions of " + value + " not inventoried");
            return v;
        }
        // An opaque boolean: a comparison outcome that cannot be modeled exactly is
        // still a boolean. SootUp reports the JVM comparison-result type as int, so
        // typing via value.getType() would yield a BITVECTOR guard; force BOOL.
        String opaqueBool(Value value) {
            String v = ssa(type("boolean"));
            add(instruction("OPAQUE_OP", false).setResultValueId(v).setPayload(opaque(value.toString())).build());
            diagnostic(id, "UNSUPPORTED_INSTRUCTION", "SYMBOLIC", "Opaque boolean comparison " + value);
            return v;
        }

        void emit() {
            var params = new ArrayList<String>();
            if (!method.isStatic()) params.add(type(method.getSignature().getDeclClassType().toString()));
            params.addAll(method.getParameterTypes().stream().map(t -> type(t.toString())).toList());
            if (!method.getReturnType().toString().equals("void")) returnTypes.add(type(method.getReturnType().toString()));
            String tid = functionType(id, params, returnTypes);
            var symbol = Cir.SymbolDef.newBuilder().setId(id + "/symbol").setName(method.getName())
                .setQualifiedName(method.getSignature().getDeclClassType() + "." + method.getName()).setKind("FUNCTION").setTypeId(tid);
            if (source != null) symbol.addSpans(Sources.span(source.file(), source.node()));
            add(symbol.build());
            for (int i = 0; i < params.size(); i++) {
                String v = id + "/p" + i;
                paramIds.add(v);
                int index = i - (method.isStatic() ? 0 : 1);
                String name = index < 0 ? "this" : source == null ? "arg" + index : source.node().getParameter(index).getNameAsString();
                add(Cir.ValueDef.newBuilder().setId(v).setFunctionId(id).setTypeId(params.get(i))
                    .setRole("PARAM").setName(name).setParamIndex(i).build());
            }
            if (!method.isConcrete()) {
                add(Cir.FunctionDef.newBuilder().setId(id).setSymbolId(id + "/symbol")
                    .addAllParameterValueIds(paramIds).addAllReturnTypeIds(returnTypes).setIsExternal(true).build());
                return;
            }
            body = method.getBody();
            List<Stmt> statements = body.getStmts();
            blockCount = statements.size();
            for (int i = 0; i < statements.size(); i++) {
                Stmt s = statements.get(i);
                blocks.put(s, id + "/b" + i);
                s.getDef().ifPresent(v -> definitions.merge(v, 1, Integer::sum));
            }
            // Bind parameters before lowering, regardless of Jimple traversal order.
            for (Stmt s : statements) if (s instanceof JIdentityStmt identity) {
                if (identity.getRightOp() instanceof JParameterRef parameter)
                    values.put(identity.getLeftOp(), paramIds.get(parameter.getIndex() + (method.isStatic() ? 0 : 1)));
                else if (identity.getRightOp() instanceof JThisRef) values.put(identity.getLeftOp(), paramIds.get(0));
            }
            // Handlers reachable through an invoke's exception edges are already wired, so
            // implicit-exception dispatch must not fire for them (and must not duplicate the
            // edge). SootUp reports exceptional successors for every statement inside a
            // protected range, including handler-internal and invoke-adjacent statements.
            for (Stmt s : statements)
                if (s.isInvokableStmt() && s.asInvokableStmt().containsInvokeExpr())
                    connectedHandlers.addAll(body.getStmtGraph().exceptionalSuccessors(s).values());
            // Allocate single-definition locals up front; assignments remain real SSA definitions.
            // Only Local left-hand sides get an SSA slot: array/field stores (`$a[i] = v`) carry a
            // non-Local LHS whose pre-allocated value would never be defined by their UNKNOWN_EFFECT
            // lowering, producing an orphaned SSA value rejected by the core (E_SSA_DOMINANCE).
            for (Stmt s : statements) s.getDef().ifPresent(v -> {
                if (v instanceof Local && definitions.get(v) == 1 && !values.containsKey(v)) {
                    String t;
                    if (s instanceof JAssignStmt a && a.getRightOp() instanceof AbstractInvokeExpr inv && a.getLeftOp() == v) {
                        // Type the slot by the invoke's declared return type: the value assigned to
                        // the local IS the invoke result, and the CIR validator requires the INVOKE
                        // result value to equal the callee's declared return type (§CIR CALL). The
                        // JVM-visible local type can be wider than the declared return (e.g. a local
                        // typed java.lang.Object receiving java.util.List from Arrays.asList), and
                        // typing the slot by the local type would make the INVOKE result mismatch and
                        // abort the whole unit with E_TYPE_MISMATCH on real code (effective-java
                        // item30 `max(argList)`).
                        t = type(inv.getType().toString());
                    } else {
                        t = localType(v);
                    }
                    values.put(v, ssa(t));
                }
            });
            add(Cir.FunctionDef.newBuilder().setId(id).setSymbolId(id + "/symbol").addAllParameterValueIds(paramIds)
                .addAllReturnTypeIds(returnTypes).setEntryBlockId(blocks.get(body.getStmtGraph().getStartingStmt())).build());
            for (int i = 0; i < statements.size(); i++)
                add(Cir.BlockDef.newBuilder().setId(blocks.get(statements.get(i))).setFunctionId(id).setOrdinal(i).build());
            for (Stmt s : statements) lower(s);
            sourceDecisions();
            if (method.getName().equals("<clinit>") || method.getModifiers().contains(MethodModifier.SYNCHRONIZED))
                diagnostic(id, "UNSUPPORTED_INSTRUCTION", "CONCURRENCY", "Class initialization/synchronized method semantics are partial");
        }

        String value(Value v) {
            if (v instanceof Local) {
                if (definitions.getOrDefault(v, 0) > 1) {
                    diagnostic(id, "UNSUPPORTED_INSTRUCTION", "SYMBOLIC", "Mutable Jimple local conservatively fresh: " + v);
                    return havoc(localType(v));
                }
                String result = values.get(v);
                if (result == null) throw new IllegalStateException("unbound local " + v);
                return result;
            }
            if (v instanceof Constant) {
                String tid = type(v.getType().toString());
                var literal = Cir.LiteralValue.newBuilder();
                if (v instanceof IntConstant n) literal.setMachineBitsBe(ByteString.copyFrom(ByteBuffer.allocate(4).putInt(n.getValue()).array()));
                else if (v instanceof LongConstant n) literal.setMachineBitsBe(ByteString.copyFrom(ByteBuffer.allocate(8).putLong(n.getValue()).array()));
                else if (v instanceof StringConstant s) literal.setStringBytes(ByteString.copyFromUtf8(s.getValue()));
                else if (v instanceof NullConstant) return opaqueValue(v); // nominal null not a CIR pointer
                else return opaqueValue(v);
                String key = tid + ":" + literal;
                if (constants.containsKey(key)) return constants.get(key);
                String result = fresh();
                constants.put(key, result);
                add(Cir.ValueDef.newBuilder().setId(result).setFunctionId(id).setTypeId(tid).setRole("CONST").setLiteral(literal).build());
                return result;
            }
            if (v instanceof AbstractBinopExpr binary) {
                String op = switch (binary.getSymbol().trim()) {
                    case "+" -> "ADD"; case "-" -> "SUB"; case "*" -> "MUL";
                    case "/" -> "DIV"; case "%" -> "REM"; case "==" -> "EQ"; case "!=" -> "NE";
                    case "<" -> "LT"; case "<=" -> "LE"; case ">" -> "GT"; case ">=" -> "GE";
                    case "&" -> "BIT_AND"; case "|" -> "BIT_OR"; case "^" -> "XOR";
                    default -> "";
                };
                if (op.isEmpty()) return opaqueValue(v);
                String left = value(binary.getOp1()), right = value(binary.getOp2());
                // JVM promotes every byte/short/char operand to int before arithmetic
                // and comparison; model that explicitly so `byte & 0xff`, `char > x`,
                // `short + int` operate on java:int on both sides (the CIR core
                // requires identical operand types for BITVECTOR ops).
                boolean arithmetic = Set.of("ADD", "SUB", "MUL", "DIV", "REM", "BIT_AND", "BIT_OR", "XOR").contains(op);
                boolean order = Set.of("LT", "LE", "GT", "GE").contains(op);
                boolean equality = op.equals("EQ") || op.equals("NE");
                if (arithmetic || order) {
                    left = widenToInt(left);
                    right = widenToInt(right);
                }
                String lhs = valueTypes.get(left), rhs = valueTypes.get(right);
                boolean modeled = true;
                if (arithmetic || order)
                    modeled = isBitvector(lhs) && isBitvector(rhs);
                else if (equality)
                    modeled = !isFloat(lhs) && !isFloat(rhs) && lhs.equals(rhs);
                // A comparison that cannot be modeled exactly (e.g. `ref == null`,
                // `flag == 0` after a call, a ternary on a non-boolean) is still a
                // boolean outcome. Emitting it as an opaque `int` (SootUp reports the
                // JVM comparison result type as int) would produce a BITVECTOR BRANCH
                // guard that the core rejects (E_TYPE_MISMATCH). Fall back to an opaque
                // BOOL value instead: the guard stays a boolean whose value is unknown.
                if (!modeled && (arithmetic || order || equality)) return opaqueBool(v);
                if (!modeled) return opaqueValue(v);
                String result = ssa(type((equality || order) ? "boolean" : v.getType().toString()));
                var inst = instruction(op, false).setResultValueId(result).addOperandValueIds(left).addOperandValueIds(right);
                if (Set.of("ADD", "SUB", "MUL", "DIV", "REM").contains(op))
                    inst.setPayload(Cir.OperationPayload.newBuilder().setBinary(Cir.BinarySemantics.newBuilder()
                        .setOperator(op).setOverflowMode("WRAP").setSignedness("SIGNED")));
                add(inst.build());
                if (op.equals("DIV") || op.equals("REM")) diagnostic(id, "EXCEPTION_FLOW_PARTIAL", "INVENTORY",
                    "ArithmeticException not inventoried for Jimple " + binary);
                return result;
            }
            return opaqueValue(v);
        }

        void lower(Stmt s) {
            block = blocks.get(s);
            instruction = 0;
            var successors = body.getStmtGraph().successors(s);
            if (s.isInvokableStmt() && s.asInvokableStmt().containsInvokeExpr()) {
                invoke(s, s.asInvokableStmt().getInvokeExpr().orElseThrow(), successors);
                return;
            }
            if (s instanceof JIfStmt branch) {
                String guard = value(branch.getCondition());
                var inst = instruction("BRANCH", true).addOperandValueIds(guard);
                add(inst.build());
                edge(s, inst.getId(), successors.get(1), 0, "TRUE", "BRANCH_TRUE", guard, false, List.of(), "", false);
                edge(s, inst.getId(), successors.get(0), 1, "FALSE", "BRANCH_FALSE", guard, true, List.of(), "", false);
                return;
            }
            if (s instanceof JSwitchStmt sw) {
                // JVM lowers every switch tag to int (byte/short/char are widened);
                // SootUp emits the case constants as IntConstant. Keep the tag on the
                // same int type so tag.type_id == const.type_id (CIR SWITCH rule).
                String key = widenToInt(value(sw.getKey()));
                var inst = instruction("SWITCH", true).addOperandValueIds(key);
                // Constants must precede the terminator in the stream, but are not instructions.
                var groups = new LinkedHashMap<Stmt, List<String>>();
                for (int i = 0; i < sw.getValues().size(); i++)
                    groups.computeIfAbsent(successors.get(i), unused -> new ArrayList<>()).add(value(sw.getValues().get(i)));
                add(inst.build());
                int index = 0;
                for (var group : groups.entrySet()) {
                    edge(s, inst.getId(), group.getKey(), index, "CASE:" + index, "SWITCH_CASE", "", false, group.getValue(), "", false);
                    index++;
                }
                edge(s, inst.getId(), successors.get(successors.size() - 1), index, "DEFAULT", "SWITCH_DEFAULT", "", false, List.of(), "", false);
                return;
            }
            if (s instanceof JReturnStmt ret) {
                String v = coerce(value(ret.getOp()), returnTypes.get(0));
                add(instruction("RETURN", true).addOperandValueIds(v).build());
                return;
            }
            if (s instanceof JReturnVoidStmt) { add(instruction("RETURN", true).build()); return; }
            if (s instanceof JThrowStmt thr) {
                String v = value(thr.getOp());
                add(instruction("THROW", true).addOperandValueIds(v).build());
                if (!body.getStmtGraph().exceptionalSuccessors(s).isEmpty())
                    diagnostic(id, "EXCEPTION_FLOW_PARTIAL", "INVENTORY", "Explicit throw dispatch is not modeled");
                return;
            }
            if (s instanceof JIdentityStmt identity) {
                if (!(identity.getRightOp() instanceof JParameterRef) && !(identity.getRightOp() instanceof JThisRef)) {
                    String result = values.get(identity.getLeftOp());
                    if (result == null) result = ssa(localType(identity.getLeftOp()));
                    add(instruction("HAVOC", false).setResultValueId(result).build());
                }
            } else if (s instanceof JAssignStmt assign && assign.getLeftOp() instanceof Local) {
                String rhs = value(assign.getRightOp());
                String result = definitions.get(assign.getLeftOp()) == 1 ? values.get(assign.getLeftOp()) : ssa(localType(assign.getLeftOp()));
                rhs = coerce(rhs, valueTypes.get(result));
                add(instruction("COPY", false).setResultValueId(result).addOperandValueIds(rhs).build());
            } else if (!(s instanceof JGotoStmt) && !(s instanceof JNopStmt)) {
                add(instruction("UNKNOWN_EFFECT", false).setPayload(opaque(s.toString())).build());
                diagnostic(id, "UNSUPPORTED_INSTRUCTION", "EFFECTS", s.toString());
                diagnostic(id, "EXCEPTION_FLOW_PARTIAL", "INVENTORY", "Implicit exceptions omitted: " + s);
            }
            // A non-invoke statement inside a protected range can still raise (array
            // load/store, arithmetic, cast, field access). SootUp reports those handlers;
            // if none of them has an incoming edge the handler chain would be dead and its
            // pre-allocated caught-exception SSA value would have no dominating definition,
            // making the core reject the whole unit with E_SSA_DOMINANCE (real projects:
            // json-simple Yylex.zzScanError). The CIR models exception edges only on INVOKE
            // terminators, so route not-yet-connected handlers through a conservative dispatch
            // switch on an opaque tag: CASE per handler (original exception-table order),
            // DEFAULT to the normal successor. Every outcome stays possible; handlers remain
            // reachable and dominated, and already-connected handlers are left alone.
            var exceptional = new ArrayList<>(body.getStmtGraph().exceptionalSuccessors(s).entrySet());
            if (!exceptional.isEmpty() && exceptional.stream().anyMatch(e -> !connectedHandlers.contains(e.getValue()))) {
                exceptional.sort(Comparator.comparingInt(e -> {
                    int n = handlers.indexOf(e.getKey().toString());
                    return n < 0 ? Integer.MAX_VALUE : n;
                }));
                String tag = havoc(type("int"));
                var inst = instruction("SWITCH", true).addOperandValueIds(tag);
                add(inst.build());
                int index = 0;
                for (var handler : exceptional) {
                    connectedHandlers.add(handler.getValue());
                    edge(s, inst.getId(), handler.getValue(), index, "CASE:" + index, "SWITCH_CASE",
                        "", false, List.of(value(IntConstant.getInstance(index))), "", false);
                    index++;
                }
                if (successors.size() != 1)
                    throw new IllegalStateException("unsupported throwing Jimple successor count: " + s);
                edge(s, inst.getId(), successors.get(0), index, "DEFAULT", "SWITCH_DEFAULT", "", false, List.of(), "", false);
                diagnostic(id, "EXCEPTION_FLOW_PARTIAL", "SYMBOLIC",
                    "Implicit exceptions of non-invoke statement over-approximated by dispatch switch: " + s);
            } else if (successors.size() == 1) {
                var jump = instruction("JUMP", true);
                add(jump.build());
                edge(s, jump.getId(), successors.get(0), 0, "NEXT", "UNCONDITIONAL", "", false, List.of(), "", false);
            } else throw new IllegalStateException("unsupported Jimple successor count: " + s);
        }

        void edge(Stmt sourceStmt, String instructionId, Stmt target, int ordinal, String label, String kind,
                  String guard, boolean negated, List<String> cases, String filter, boolean catchAll) {
            edgeTo(sourceStmt, instructionId, blocks.get(target), ordinal, label, kind, guard, negated, cases, filter, catchAll);
        }
        void edgeTo(Stmt sourceStmt, String instructionId, String target, int ordinal, String label, String kind,
                    String guard, boolean negated, List<String> cases, String filter, boolean catchAll) {
            var edge = Cir.ControlEdgeDef.newBuilder().setId(instructionId + "/e" + ordinal).setSourceBlockId(block)
                .setTargetBlockId(target).setTerminatorInstructionId(instructionId).setOutcomeOrdinal(ordinal)
                .setOutcomeLabel(label).setEdgeKind(kind).setGuardValueId(guard).setGuardNegated(negated)
                .addAllCaseValueIds(cases).setExceptionFilterTypeId(filter).setExceptionCatchAll(catchAll).build();
            add(edge);
            statementEdges.computeIfAbsent(sourceStmt, unused -> new ArrayList<>()).add(edge);
            edges++;
        }

        void invoke(Stmt stmt, AbstractInvokeExpr call, List<Stmt> normal) {
            boolean reflective = call.getMethodSignature().getDeclClassType().toString().equals("java.lang.Class")
                && Set.of("forName", "newInstance", "getMethod", "getDeclaredMethod").contains(call.getMethodSignature().getName());
            String dispatch = reflective ? "REFLECTIVE" : call instanceof JStaticInvokeExpr || call instanceof JSpecialInvokeExpr ? "STATIC"
                : call instanceof JInterfaceInvokeExpr ? "INTERFACE" : call instanceof JVirtualInvokeExpr ? "VIRTUAL" : "DYNAMIC";
            var argTypes = new ArrayList<String>();
            var args = new ArrayList<String>();
            if (call instanceof AbstractInstanceInvokeExpr instance) {
                String receiver = type(call.getMethodSignature().getDeclClassType().toString());
                argTypes.add(receiver);
                args.add(coerce(value(instance.getBase()), receiver));
            }
            for (int i = 0; i < call.getArgs().size(); i++) {
                String parameter = type(call.getMethodSignature().getParameterTypes().get(i).toString());
                argTypes.add(parameter);
                args.add(coerce(value(call.getArg(i)), parameter));
            }
            List<String> results = call.getType().toString().equals("void") ? List.of() : List.of(type(call.getType().toString()));
            boolean internal = projectMethods.contains(call.getMethodSignature().toString());
            String target = (internal ? "java:" : "java:external:") + call.getMethodSignature();
            String tid = functionType(target, argTypes, results);
            String callee;
            if (dispatch.equals("STATIC")) {
                if (stubs.add(target)) {
                    if (!internal) {
                    add(Cir.SymbolDef.newBuilder().setId(target + "/symbol").setName(call.getMethodSignature().getName())
                        .setQualifiedName(call.getMethodSignature().toString()).setKind("FUNCTION").setTypeId(tid).build());
                    var parameters = new ArrayList<String>();
                    for (int i = 0; i < argTypes.size(); i++) {
                        String p = target + "/p" + i;
                        add(Cir.ValueDef.newBuilder().setId(p).setFunctionId(target).setTypeId(argTypes.get(i)).setRole("PARAM").setParamIndex(i).build());
                        parameters.add(p);
                    }
                    add(Cir.FunctionDef.newBuilder().setId(target).setSymbolId(target + "/symbol").setIsExternal(true)
                        .addAllParameterValueIds(parameters).addAllReturnTypeIds(results).build());
                    }
                    add(Cir.ValueDef.newBuilder().setId(target + "/global").setTypeId(tid).setRole("GLOBAL").setSymbolId(target + "/symbol").build());
                }
                callee = target + "/global";
            } else {
                callee = havoc(tid);
                diagnostic(id, "CALL_TARGET_UNRESOLVED", "CALLGRAPH", "UNRESOLVED " + dispatch + ": " + call.getMethodSignature());
            }
            var inst = instruction("INVOKE", true).addOperandValueIds(callee).addAllOperandValueIds(args)
                .setPayload(Cir.OperationPayload.newBuilder().setCall(Cir.CallSemantics.newBuilder().setDispatchKind(dispatch)
                    .setMayRaise(true).setMayMutateMemory(true).setWorldClosed(false)));
            if (!results.isEmpty()) {
                String result = stmt instanceof JAssignStmt a && definitions.getOrDefault(a.getLeftOp(), 0) == 1
                    ? values.get(a.getLeftOp()) : ssa(results.get(0));
                inst.setResultValueId(result);
            }
            add(inst.build());
            if (dispatch.equals("STATIC")) add(Cir.CallTargetDef.newBuilder().setCallInstructionId(inst.getId())
                .setTargetFunctionId(target).setResolutionKind("STATIC").build());
            if (normal.size() != 1) throw new IllegalStateException("invoke missing normal successor");
            edge(stmt, inst.getId(), normal.get(0), 0, "NORMAL", "NORMAL", "", false, List.of(), "", false);
            var exceptional = new ArrayList<>(body.getStmtGraph().exceptionalSuccessors(stmt).entrySet());
            if (exceptional.stream().anyMatch(e -> !handlers.contains(e.getKey().toString())))
                diagnostic(id, "EXCEPTION_FLOW_PARTIAL", "INVENTORY", "Handler has no matching original exception table entry");
            exceptional.sort(Comparator.comparingInt(e -> {
                int n = handlers.indexOf(e.getKey().toString());
                return n < 0 ? Integer.MAX_VALUE : n;
            }));
            int ordinal = 1;
            boolean catchesAll = false;
            for (var handler : exceptional) {
                String name = handler.getKey().toString();
                boolean all = name.equals("java.lang.Throwable");
                catchesAll |= all;
                edge(stmt, inst.getId(), handler.getValue(), ordinal++, "EXCEPTION", "EXCEPTION", "", false, List.of(), all ? "" : type(name), all);
            }
            if (!catchesAll) {
                String savedBlock = block;
                int savedInstruction = instruction;
                String propagation = inst.getId() + "/propagate";
                add(Cir.BlockDef.newBuilder().setId(propagation).setFunctionId(id)
                    .setOrdinal(blockCount++).build());
                edgeTo(stmt, inst.getId(), propagation, ordinal, "EXCEPTION", "EXCEPTION", "", false, List.of(), "", true);
                block = propagation;
                instruction = 0;
                String exception = havoc(type("java.lang.Throwable"));
                add(instruction("THROW", true).addOperandValueIds(exception).build());
                block = savedBlock;
                instruction = savedInstruction;
            }
            diagnostic(id, "EXCEPTION_FLOW_PARTIAL", "EFFECTS", "Exception subtype guards overapproximated; original handler precedence retained");
            diagnostic(id, "UNSUPPORTED_INSTRUCTION", "EFFECTS", "Open-world invocation and class initialization effects");
        }

        void sourceDecisions() {
            if (source == null) return;
            int index = 0;
            for (Node n : Sources.decisions(source.node())) {
                String decision = id + "/d" + index++;
                String kind;
                Node condition = n;
                var labels = new ArrayList<String>();
                if (n instanceof IfStmt branch) { kind = "IF"; condition = branch.getCondition(); labels.addAll(List.of("TRUE", "FALSE")); }
                else if (n instanceof SwitchStmt sw) {
                    kind = "SWITCH"; condition = sw.getSelector();
                    int cases = 0;
                    // Colon entries with no statements share the next executable case.
                    boolean pending = false;
                    for (SwitchEntry entry : sw.getEntries()) {
                        if (entry.getLabels().isEmpty()) { labels.add("DEFAULT"); pending = false; }
                        else pending = true;
                        if (pending && !entry.getStatements().isEmpty()) { labels.add("CASE:" + cases++); pending = false; }
                    }
                } else if (n instanceof TryStmt) { kind = "TRY_CATCH"; labels.addAll(List.of("SUCCESS", "EXCEPTION")); }
                else if (n instanceof ConditionalExpr expression) { kind = "TERNARY"; condition = expression.getCondition(); labels.addAll(List.of("TRUE", "FALSE")); }
                else if (n instanceof BinaryExpr) { kind = "LOGICAL_OPERAND"; labels.addAll(List.of("TRUE", "FALSE")); }
                else { kind = "LOOP_CONDITION"; labels.addAll(List.of("ENTER_BODY", "EXIT")); }
                // Position alone supplies provenance, not an exact activation attestation.
                int line = condition.getBegin().orElseThrow().line;
                var matches = body.getStmts().stream().filter(s ->
                    s.getPositionInfo().getStmtPosition().getFirstLine() == line
                    && (kind.equals("IF") ? s instanceof JIfStmt : kind.equals("SWITCH") && s instanceof JSwitchStmt)).toList();
                boolean exact = kind.equals("IF") && matches.size() == 1
                    && Sources.decisions(source.node()).stream().filter(d -> d instanceof IfStmt branch
                        && branch.getCondition().getBegin().orElseThrow().line == line).count() == 1
                    && condition instanceof BinaryExpr binary && matches.get(0) instanceof JIfStmt branch
                    && exactCondition(binary, branch.getCondition());
                Cir.Span span = Sources.span(source.file(), condition);
                if (n instanceof TryStmt) span = span.toBuilder().setEndLine(span.getStartLine())
                    .setEndColumn(span.getStartColumn() + 3).build();
                var definition = Cir.SourceDecisionDef.newBuilder().setId(decision).setFunctionId(id)
                    .setDecisionKind(kind).setSpan(span);
                for (String label : labels) definition.addOutcomes(Cir.SourceOutcomeDef.newBuilder()
                    .setId(decision + "/" + label).setLabel(label).setActivationMappingComplete(exact));
                add(definition.build());
                decisions++;
                if (matches.size() == 1) {
                    for (var edge : statementEdges.getOrDefault(matches.get(0), List.of())) {
                        String label = edge.getOutcomeLabel();
                        if (kind.equals("IF") && condition instanceof BinaryExpr binary && matches.get(0) instanceof JIfStmt jif) {
                            String sourceOperator = binary.getOperator().asString();
                            String irOperator = jif.getCondition().getSymbol().trim();
                            // Bytecode emits a branch over the source body (inverse comparison).
                            if (Map.of("<", ">=", "<=", ">", ">", "<=", ">=", "<", "==", "!=", "!=", "==")
                                .getOrDefault(sourceOperator, "").equals(irOperator))
                                label = label.equals("TRUE") ? "FALSE" : "TRUE";
                        }
                        if (labels.contains(label)) {
                            add(Cir.SourceIrLinkDef.newBuilder().setSourceOutcomeId(decision + "/" + label)
                                .setControlEdgeId(edge.getId()).setMappingKind(kind.equals("SWITCH") ? "SWITCH_CASE" : "DIRECT")
                                .setQuality(exact ? "EXACT" : "APPROX").setInverted(!label.equals(edge.getOutcomeLabel())).build());
                            if (exact) add(Cir.SourceOutcomeActivationCaseDef.newBuilder()
                                .setSourceOutcomeId(decision + "/" + label).setCaseOrdinal(0)
                                .addOrderedControlEdgeIds(edge.getId()).setQuality("EXACT").build());
                        }
                    }
                }
                if (!exact) diagnostic(id, "SOURCE_MAPPING_PARTIAL", "SYMBOLIC", "Source activation not attested for " + kind);
                if (n instanceof TryStmt t && t.getFinallyBlock().isPresent())
                    diagnostic(id, "EXCEPTION_FLOW_PARTIAL", "INVENTORY", "Finally duplicated bytecode retained; implicit exceptions/cleanup propagation not fully inventoried");
            }
        }

        boolean exactCondition(BinaryExpr sourceExpression, AbstractConditionExpr ir) {
            String sourceOperator = sourceExpression.getOperator().asString();
            String inverse = Map.of("<", ">=", "<=", ">", ">", "<=", ">=", "<", "==", "!=", "!=", "==").get(sourceOperator);
            if (inverse == null || !(sourceOperator.equals(ir.getSymbol().trim()) || inverse.equals(ir.getSymbol().trim()))) return false;
            String left = sourceKey(sourceExpression.getLeft()), right = sourceKey(sourceExpression.getRight());
            return left != null && right != null && left.equals(irKey(ir.getOp1())) && right.equals(irKey(ir.getOp2()));
        }
        String sourceKey(Expression expression) {
            if (expression instanceof NameExpr name) {
                for (int i = 0; i < source.node().getParameters().size(); i++)
                    if (source.node().getParameter(i).getNameAsString().equals(name.getNameAsString()))
                        return paramIds.get(i + (method.isStatic() ? 0 : 1));
            }
            try {
                if (expression instanceof IntegerLiteralExpr literal) return "int:" + literal.asNumber().intValue();
                if (expression instanceof UnaryExpr unary && unary.getOperator() == UnaryExpr.Operator.MINUS
                    && unary.getExpression() instanceof IntegerLiteralExpr literal)
                    return "int:" + -literal.asNumber().intValue();
            } catch (NumberFormatException ignored) { /* no attestation for unsupported literals */ }
            return null;
        }
        String irKey(Value value) {
            if (value instanceof IntConstant integer) return "int:" + integer.getValue();
            if (value instanceof Local && definitions.getOrDefault(value, 0) == 1) return values.get(value);
            return null;
        }
    }
}
