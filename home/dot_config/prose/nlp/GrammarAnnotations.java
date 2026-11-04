import com.fasterxml.jackson.databind.DeserializationFeature;
import com.fasterxml.jackson.databind.PropertyNamingStrategies;
import com.fasterxml.jackson.databind.json.JsonMapper;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.time.LocalDate;
import java.time.ZoneOffset;
import java.util.ArrayList;
import java.util.HexFormat;
import java.util.List;
import org.languagetool.JLanguageTool;
import org.languagetool.Languages;
import org.languagetool.LtBuildInfo;
import org.languagetool.rules.RuleMatch;

public final class GrammarAnnotations {
    private static final int SCHEMA = 1;

    public record Segment(String id, String text) {}
    public record Request(int schemaVersion, String language, List<Segment> segments) {}
    public record Match(
        int startUtf16,
        int endUtf16,
        String rule,
        String message,
        String category,
        String issueType,
        boolean spelling,
        List<String> replacements
    ) {}
    public record Annotation(String id, String textSha256, List<Match> matches) {}
    public record Response(
        int schemaVersion,
        String engineVersion,
        String javaVersion,
        String referenceStartDate,
        String referenceEndDate,
        String language,
        List<String> activeRules,
        List<Annotation> annotations
    ) {}

    private GrammarAnnotations() {}

    public static void main(String[] arguments) throws Exception {
        if (arguments.length != 2) {
            throw new IllegalArgumentException("request and response paths are required");
        }
        var mapper = JsonMapper.builder()
            .enable(DeserializationFeature.FAIL_ON_UNKNOWN_PROPERTIES)
            .enable(DeserializationFeature.FAIL_ON_MISSING_CREATOR_PROPERTIES)
            .enable(DeserializationFeature.FAIL_ON_NULL_CREATOR_PROPERTIES)
            .propertyNamingStrategy(PropertyNamingStrategies.SNAKE_CASE)
            .build();
        Request request = mapper.readValue(Files.readAllBytes(Path.of(arguments[0])), Request.class);
        if (request.schemaVersion() != SCHEMA || request.segments().isEmpty()) {
            throw new IllegalArgumentException("invalid schema or empty request");
        }
        var tool = new JLanguageTool(Languages.getLanguageForShortCode(request.language()));
        LocalDate referenceDate = LocalDate.now(ZoneOffset.UTC);
        var annotations = new ArrayList<Annotation>();
        for (Segment segment : request.segments()) {
            if (segment.id().isEmpty() || segment.text().isEmpty()) {
                throw new IllegalArgumentException("empty segment identity or text");
            }
            var matches = new ArrayList<Match>();
            for (RuleMatch match : tool.check(segment.text())) {
                matches.add(new Match(
                    match.getFromPos(),
                    match.getToPos(),
                    match.getSpecificRuleId(),
                    match.getMessage(),
                    match.getRule().getCategory().getId().toString(),
                    match.getRule().getLocQualityIssueType().toString(),
                    match.getRule().isDictionaryBasedSpellingRule(),
                    List.copyOf(match.getSuggestedReplacements())
                ));
            }
            String hash = HexFormat.of().formatHex(
                MessageDigest.getInstance("SHA-256").digest(segment.text().getBytes(StandardCharsets.UTF_8))
            );
            annotations.add(new Annotation(segment.id(), hash, List.copyOf(matches)));
        }
        Response response = new Response(
            SCHEMA,
            LtBuildInfo.OS.getVersion(),
            System.getProperty("java.runtime.version"),
            referenceDate.toString(),
            LocalDate.now(ZoneOffset.UTC).toString(),
            request.language(),
            tool.getAllActiveRules().stream().map(rule -> rule.getId()).sorted().toList(),
            List.copyOf(annotations)
        );
        mapper.writeValue(Path.of(arguments[1]).toFile(), response);
    }
}
