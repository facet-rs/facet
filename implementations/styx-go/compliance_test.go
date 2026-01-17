package styx

import (
	"bytes"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"testing"
)

// TestCompliance runs all .styx files in the compliance corpus through both
// the Go parser and the Rust reference implementation, comparing their s-expression output.
func TestCompliance(t *testing.T) {
	// Find the compliance corpus relative to this test file
	corpusPath := findCorpusPath(t)

	// Find styx-cli for reference output
	styxCLI := findStyxCLI(t)

	// Collect all .styx files
	var files []string
	err := filepath.Walk(corpusPath, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return err
		}
		if !info.IsDir() && strings.HasSuffix(path, ".styx") {
			files = append(files, path)
		}
		return nil
	})
	if err != nil {
		t.Fatalf("failed to walk corpus: %v", err)
	}

	sort.Strings(files)

	for _, file := range files {
		relPath, _ := filepath.Rel(corpusPath, file)
		t.Run(relPath, func(t *testing.T) {
			compareOutput(t, file, styxCLI)
		})
	}
}

func findCorpusPath(t *testing.T) string {
	// Try relative paths from the test file location
	candidates := []string{
		"../../compliance/corpus",
		"../../../compliance/corpus",
	}

	for _, c := range candidates {
		abs, err := filepath.Abs(c)
		if err != nil {
			continue
		}
		if info, err := os.Stat(abs); err == nil && info.IsDir() {
			return abs
		}
	}

	t.Fatal("could not find compliance corpus directory")
	return ""
}

func findStyxCLI(t *testing.T) string {
	// Try local build first (prefer local changes over installed version)
	candidates := []string{
		"../../target/debug/styx",
		"../../target/release/styx",
		"../../../target/debug/styx",
		"../../../target/release/styx",
	}

	for _, c := range candidates {
		abs, err := filepath.Abs(c)
		if err != nil {
			continue
		}
		if _, err := os.Stat(abs); err == nil {
			return abs
		}
	}

	// Fall back to PATH
	if path, err := exec.LookPath("styx"); err == nil {
		return path
	}

	t.Skip("styx-cli not found - run 'cargo build' first")
	return ""
}

func compareOutput(t *testing.T, file string, styxCLI string) {
	content, err := os.ReadFile(file)
	if err != nil {
		t.Fatalf("failed to read file: %v", err)
	}

	// Get Go parser output
	goOutput := getGoOutput(string(content))

	// Get Rust reference output
	rustOutput := getRustOutput(t, file, styxCLI)

	// Normalize both outputs for comparison
	goNorm := normalizeOutput(goOutput)
	rustNorm := normalizeOutput(rustOutput)

	if goNorm != rustNorm {
		t.Errorf("output mismatch\n--- Go output ---\n%s\n--- Rust output ---\n%s",
			goOutput, rustOutput)
	}
}

func getGoOutput(content string) string {
	doc, err := Parse(content)
	if err != nil {
		if pe, ok := err.(*ParseError); ok {
			return formatErrorSexp(pe)
		}
		return fmt.Sprintf("(error [-1, -1] \"parse error: %s\")", escapeStringSexp(err.Error()))
	}
	return formatDocumentSexp(doc)
}

func getRustOutput(t *testing.T, file string, styxCLI string) string {
	cmd := exec.Command(styxCLI, "@tree", "--format", "sexp", file)
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr

	err := cmd.Run()
	if err != nil {
		// Check if stderr has an error message (parse error case)
		if stderr.Len() > 0 {
			// Extract error info from stderr
			return extractErrorFromStderr(stderr.String())
		}
		t.Fatalf("styx-cli failed: %v\nstderr: %s", err, stderr.String())
	}

	return stdout.String()
}

func extractErrorFromStderr(stderr string) string {
	// Parse error messages like "error: parse error at 9-10: expected a value"
	// And convert to sexp format
	lines := strings.Split(stderr, "\n")
	for _, line := range lines {
		if strings.HasPrefix(line, "error: parse error at ") {
			// Extract span from "parse error at X-Y: message"
			re := regexp.MustCompile(`parse error at (\d+)-(\d+): (.+)`)
			if m := re.FindStringSubmatch(line[7:]); m != nil {
				return fmt.Sprintf("(error [%s, %s] \"parse error at %s-%s: %s\")", m[1], m[2], m[1], m[2], escapeStringSexp(m[3]))
			}
		}
	}
	return fmt.Sprintf("(error [-1, -1] \"%s\")", escapeStringSexp(strings.TrimSpace(stderr)))
}

func normalizeOutput(output string) string {
	// Remove file comments and normalize whitespace
	lines := strings.Split(output, "\n")
	var result []string
	for _, line := range lines {
		trimmed := strings.TrimSpace(line)
		if strings.HasPrefix(trimmed, "; file:") {
			continue
		}
		if trimmed == "" {
			continue
		}
		result = append(result, trimmed)
	}
	return strings.Join(result, "\n")
}

func escapeStringSexp(s string) string {
	s = strings.ReplaceAll(s, "\\", "\\\\")
	s = strings.ReplaceAll(s, "\"", "\\\"")
	s = strings.ReplaceAll(s, "\n", "\\n")
	s = strings.ReplaceAll(s, "\r", "\\r")
	s = strings.ReplaceAll(s, "\t", "\\t")
	return s
}

func formatErrorSexp(err *ParseError) string {
	escapedMsg := escapeStringSexp(err.Message)
	return fmt.Sprintf("(error [%d, %d] \"parse error at %d-%d: %s\")", err.Span.Start, err.Span.End, err.Span.Start, err.Span.End, escapedMsg)
}

func formatDocumentSexp(doc *Document) string {
	if len(doc.Entries) == 0 {
		return "(document [-1, -1]\n)"
	}
	var entries []string
	for _, entry := range doc.Entries {
		entries = append(entries, formatEntrySexp(entry, 1))
	}
	return fmt.Sprintf("(document [-1, -1]\n%s\n)", strings.Join(entries, "\n"))
}

func formatEntrySexp(entry *Entry, indent int) string {
	prefix := strings.Repeat("  ", indent)
	keyStr := formatValueSexp(entry.Key, indent+1)
	valueStr := formatValueSexp(entry.Value, indent+1)
	return fmt.Sprintf("%s(entry\n%s  %s\n%s  %s)", prefix, prefix, keyStr, prefix, valueStr)
}

func formatValueSexp(value *Value, indent int) string {
	prefix := strings.Repeat("  ", indent)

	// Unit value (no tag, no payload)
	if value.Tag == nil && value.PayloadKind == PayloadNone {
		return fmt.Sprintf("(unit [%d, %d])", value.Span.Start, value.Span.End)
	}

	// Tag only (no payload)
	if value.Tag != nil && value.PayloadKind == PayloadNone {
		return fmt.Sprintf("(tag [%d, %d] \"%s\")", value.Span.Start, value.Span.End, value.Tag.Name)
	}

	// Tag with payload
	if value.Tag != nil && value.PayloadKind != PayloadNone {
		payloadStr := formatPayloadSexp(value, indent+1)
		return fmt.Sprintf("(tag [%d, %d] \"%s\"\n%s  %s)", value.Span.Start, value.Span.End, value.Tag.Name, prefix, payloadStr)
	}

	// Just payload
	if value.PayloadKind != PayloadNone {
		return formatPayloadSexp(value, indent)
	}

	return fmt.Sprintf("(unit [%d, %d])", value.Span.Start, value.Span.End)
}

func formatPayloadSexp(value *Value, indent int) string {
	prefix := strings.Repeat("  ", indent)

	switch value.PayloadKind {
	case PayloadScalar:
		escaped := escapeStringSexp(value.Scalar.Text)
		return fmt.Sprintf("(scalar [%d, %d] %s \"%s\")", value.Scalar.Span.Start, value.Scalar.Span.End, value.Scalar.Kind, escaped)

	case PayloadSequence:
		seq := value.Sequence
		if len(seq.Items) == 0 {
			return fmt.Sprintf("(sequence [%d, %d])", seq.Span.Start, seq.Span.End)
		}
		var items []string
		for _, item := range seq.Items {
			items = append(items, fmt.Sprintf("%s  %s", prefix, formatValueSexp(item, indent+1)))
		}
		return fmt.Sprintf("(sequence [%d, %d]\n%s)", seq.Span.Start, seq.Span.End, strings.Join(items, "\n"))

	case PayloadObject:
		obj := value.Object
		if len(obj.Entries) == 0 {
			return fmt.Sprintf("(object [%d, %d] %s)", obj.Span.Start, obj.Span.End, obj.Separator)
		}
		var entries []string
		for _, entry := range obj.Entries {
			entries = append(entries, formatEntrySexp(entry, indent+1))
		}
		return fmt.Sprintf("(object [%d, %d] %s\n%s\n%s)", obj.Span.Start, obj.Span.End, obj.Separator, strings.Join(entries, "\n"), prefix)
	}

	return "(unknown)"
}
