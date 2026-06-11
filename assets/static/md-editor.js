// Minimal, dependency-free Markdown editor enhancement for the demo app.
//
// fracture-core ships the blog admin templates but deliberately does not bundle
// an editor — templates mark their textarea with `data-md-editor` and the
// consuming app provides this file (see README "Markdown editor hook").
//
// This implementation progressively enhances each `[data-md-editor]` textarea
// with a small formatting toolbar. It inserts Markdown syntax around the
// current selection; it does NOT render HTML, so there is no XSS surface and
// nothing inline that would violate the Content-Security-Policy.

(function () {
    "use strict";

    // Wrap the current selection (or insert a placeholder) with `before`/`after`.
    function surround(textarea, before, after, placeholder) {
        var start = textarea.selectionStart;
        var end = textarea.selectionEnd;
        var selected = textarea.value.slice(start, end) || placeholder || "";
        var replacement = before + selected + after;
        textarea.setRangeText(replacement, start, end, "end");
        // Re-select the inner text so the user can keep typing/replacing.
        textarea.selectionStart = start + before.length;
        textarea.selectionEnd = start + before.length + selected.length;
        textarea.focus();
        // Notify any listeners (e.g. autosave) that the value changed.
        textarea.dispatchEvent(new Event("input", { bubbles: true }));
    }

    // Prefix the start of each selected line with `prefix` (for headings/lists).
    function prefixLines(textarea, prefix) {
        var start = textarea.selectionStart;
        var end = textarea.selectionEnd;
        var value = textarea.value;
        // lastIndexOf with fromIndex -1 clamps to 0 and can still match a
        // leading "\n", which would put lineStart past the caret.
        var lineStart = start > 0 ? value.lastIndexOf("\n", start - 1) + 1 : 0;
        var block = value.slice(lineStart, end);
        var lines = block.split("\n");
        var updated = lines
            .map(function (line, i) {
                // A selection ending just past a newline yields a trailing
                // empty segment; prefixing it would mark the next, unselected
                // line.
                if (i > 0 && i === lines.length - 1 && line === "") {
                    return line;
                }
                return prefix + line;
            })
            .join("\n");
        textarea.setRangeText(updated, lineStart, end, "end");
        textarea.focus();
        textarea.dispatchEvent(new Event("input", { bubbles: true }));
    }

    var BUTTONS = [
        { label: "B", title: "Bold", action: function (t) { surround(t, "**", "**", "bold text"); } },
        { label: "i", title: "Italic", action: function (t) { surround(t, "_", "_", "italic text"); } },
        { label: "H", title: "Heading", action: function (t) { prefixLines(t, "## "); } },
        { label: "“ ”", title: "Quote", action: function (t) { prefixLines(t, "> "); } },
        { label: "•", title: "List item", action: function (t) { prefixLines(t, "- "); } },
        { label: "</>", title: "Code", action: function (t) { surround(t, "`", "`", "code"); } },
        { label: "link", title: "Link", action: function (t) { surround(t, "[", "](https://)", "text"); } },
    ];

    function buildToolbar(textarea) {
        var bar = document.createElement("div");
        bar.className = "md-toolbar";
        bar.setAttribute("role", "toolbar");
        bar.setAttribute("aria-label", "Markdown formatting");

        BUTTONS.forEach(function (spec) {
            var btn = document.createElement("button");
            btn.type = "button"; // never submit the form
            btn.className = "button outline small";
            btn.textContent = spec.label;
            btn.title = spec.title;
            btn.setAttribute("aria-label", spec.title);
            btn.addEventListener("click", function () {
                spec.action(textarea);
            });
            bar.appendChild(btn);
        });
        return bar;
    }

    document.addEventListener("DOMContentLoaded", function () {
        document.querySelectorAll("textarea[data-md-editor]").forEach(function (textarea) {
            if (textarea.dataset.mdEditorReady) {
                return;
            }
            textarea.dataset.mdEditorReady = "true";
            var toolbar = buildToolbar(textarea);
            textarea.parentNode.insertBefore(toolbar, textarea);
        });
    });
})();
