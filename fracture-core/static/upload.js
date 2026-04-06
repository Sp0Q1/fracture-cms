(function () {
  "use strict";

  var ALLOWED_TYPES = [
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
    "image/svg+xml",
  ];
  var MAX_SIZE = 5 * 1024 * 1024; // 5 MiB

  function init() {
    var textareas = document.querySelectorAll("textarea[data-upload-target]");
    for (var i = 0; i < textareas.length; i++) {
      setupTextarea(textareas[i]);
    }
  }

  function setupTextarea(textarea) {
    var status = document.createElement("div");
    status.setAttribute("role", "status");
    status.setAttribute("aria-live", "polite");
    status.style.cssText =
      "font-size:0.85em;color:#666;min-height:1.2em;margin-top:4px";
    textarea.parentNode.insertBefore(status, textarea.nextSibling);

    textarea.addEventListener("dragover", function (e) {
      e.preventDefault();
      e.stopPropagation();
    });

    textarea.addEventListener("drop", function (e) {
      e.preventDefault();
      e.stopPropagation();
      var files = e.dataTransfer && e.dataTransfer.files;
      if (files && files.length > 0) {
        handleFiles(textarea, status, files);
      }
    });

    textarea.addEventListener("paste", function (e) {
      var items = e.clipboardData && e.clipboardData.items;
      if (!items) return;
      for (var i = 0; i < items.length; i++) {
        if (items[i].kind === "file") {
          var file = items[i].getAsFile();
          if (file) {
            e.preventDefault();
            handleFiles(textarea, status, [file]);
            return;
          }
        }
      }
    });
  }

  function handleFiles(textarea, status, files) {
    for (var i = 0; i < files.length; i++) {
      uploadFile(textarea, status, files[i]);
    }
  }

  function uploadFile(textarea, status, file) {
    // Client-side validation
    if (ALLOWED_TYPES.indexOf(file.type) === -1) {
      status.textContent = "Error: File type " + file.type + " is not allowed.";
      return;
    }
    if (file.size > MAX_SIZE) {
      status.textContent =
        "Error: File too large (" +
        Math.round(file.size / 1024 / 1024) +
        " MiB). Maximum is 5 MiB.";
      return;
    }

    status.textContent = "Uploading\u2026";

    var formData = new FormData();
    formData.append("file", file);

    fetch("/api/uploads", {
      method: "POST",
      body: formData,
      credentials: "same-origin",
    })
      .then(function (response) {
        if (!response.ok) {
          return response.text().then(function (text) {
            throw new Error(text || "Upload failed (status " + response.status + ")");
          });
        }
        return response.json();
      })
      .then(function (data) {
        status.textContent = "";
        var markdown = "![image](/api/uploads/" + data.pid + ")";
        insertAtCursor(textarea, markdown);
      })
      .catch(function (err) {
        status.textContent = "Upload failed: " + err.message;
      });
  }

  function insertAtCursor(textarea, text) {
    var start = textarea.selectionStart;
    var end = textarea.selectionEnd;
    var before = textarea.value.substring(0, start);
    var after = textarea.value.substring(end);
    textarea.value = before + text + after;
    textarea.selectionStart = textarea.selectionEnd = start + text.length;
    textarea.focus();
    // Trigger input event so frameworks can detect the change
    var event = new Event("input", { bubbles: true });
    textarea.dispatchEvent(event);
  }

  // Initialize when DOM is ready
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
