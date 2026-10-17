package org.ske.javaadapter;

import com.github.javaparser.*;
import com.github.javaparser.ast.*;
import com.github.javaparser.ast.body.*;
import com.github.javaparser.ast.expr.*;
import com.github.javaparser.ast.stmt.*;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;
import ske.cir.v1.Cir;

/** Source inventory is independent of the bytecode lowering. Columns are UTF-8 bytes. */
final class Sources {
    record File(String path, String text, CompilationUnit ast) {}
    record Method(File file, CallableDeclaration<?> node, String owner) {}
    final List<File> files = new ArrayList<>();
    final List<Method> methods = new ArrayList<>();

    Sources(Path root, Main.Profile profile) throws IOException {
        var parser = new JavaParser(new ParserConfiguration()
            .setLanguageLevel(ParserConfiguration.LanguageLevel.JAVA_17));
        var paths = new TreeSet<Path>();
        for (String relative : profile.source_roots()) {
            Path dir = Main.inside(root, relative);
            try (var stream = Files.walk(dir)) {
                for (Path p : stream.filter(p -> p.toString().endsWith(".java")).toList())
                    paths.add(Main.inside(root, root.relativize(p).toString()));
            }
        }
        for (Path p : paths) {
            String text = Files.readString(p);
            var result = parser.parse(text);
            if (!result.isSuccessful()) throw new IOException("SOURCE_PARSE_FAILED: " + result.getProblems());
            File f = new File(root.relativize(p).toString().replace('\\', '/'), text, result.getResult().orElseThrow());
            files.add(f);
            for (CallableDeclaration<?> n : f.ast().findAll(CallableDeclaration.class)) {
                var owner = n.findAncestor(TypeDeclaration.class).orElseThrow();
                String name = f.ast().getPackageDeclaration().map(x -> x.getNameAsString() + ".").orElse("") + owner.getNameAsString();
                methods.add(new Method(f, n, name));
            }
        }
    }

    Method method(String owner, String name, List<String> parameterTypes) {
        var candidates = methods.stream().filter(m -> m.owner().equals(owner)
            && m.node().getNameAsString().equals(name)
            && m.node().getParameters().size() == parameterTypes.size()).toList();
        if (candidates.size() == 1) return candidates.get(0);
        // Avoid attributing spans to a guessed overload.
        return candidates.stream().filter(m -> {
            for (int i = 0; i < parameterTypes.size(); i++) {
                String source = m.node().getParameter(i).getTypeAsString();
                String target = parameterTypes.get(i);
                if (!source.equals(target) && !target.endsWith("." + source)) return false;
            }
            return true;
        }).findFirst().orElse(null);
    }

    static Cir.Span span(File file, Node node) {
        var r = node.getRange().orElseThrow();
        String[] lines = file.text().split("\n", -1);
        int start = lines[r.begin.line - 1].substring(0, r.begin.column - 1).getBytes(StandardCharsets.UTF_8).length + 1;
        int end = lines[r.end.line - 1].substring(0, r.end.column).getBytes(StandardCharsets.UTF_8).length + 1;
        return Cir.Span.newBuilder().setPath(file.path()).setStartLine(r.begin.line).setStartColumn(start)
            .setEndLine(r.end.line).setEndColumn(end).build();
    }

    static List<Node> decisions(CallableDeclaration<?> method) {
        return method.stream().filter(n -> n instanceof IfStmt || n instanceof SwitchStmt
            || n instanceof TryStmt || n instanceof ConditionalExpr || n instanceof WhileStmt
            || n instanceof DoStmt || n instanceof ForStmt || n instanceof ForEachStmt
            || n instanceof BinaryExpr b && (b.getOperator() == BinaryExpr.Operator.AND
                || b.getOperator() == BinaryExpr.Operator.OR)).toList();
    }

    // Deliberately separate visitor/count from the emitter's decisions traversal.
    long count() {
        return files.stream().mapToLong(f -> {
            long count = f.ast().findAll(IfStmt.class).size() + f.ast().findAll(SwitchStmt.class).size()
                + f.ast().findAll(TryStmt.class).size() + f.ast().findAll(ConditionalExpr.class).size()
                + f.ast().findAll(WhileStmt.class).size() + f.ast().findAll(DoStmt.class).size()
                + f.ast().findAll(ForStmt.class).size() + f.ast().findAll(ForEachStmt.class).size();
            return count + f.ast().findAll(BinaryExpr.class).stream().filter(b ->
                b.getOperator() == BinaryExpr.Operator.AND || b.getOperator() == BinaryExpr.Operator.OR).count();
        }).sum();
    }
}
