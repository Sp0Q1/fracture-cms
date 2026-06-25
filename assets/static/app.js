document.addEventListener("DOMContentLoaded", function () {
    // Remember each list's sort in the browser and reapply it on return. The
    // shared list partial emits <span data-list-base="<base_url>">. With a sort
    // in the URL we save it; without one we reapply the saved sort (if any).
    (function rememberListSort() {
        var marker = document.querySelector("[data-list-base]");
        if (!marker || !window.localStorage) {
            return;
        }
        var base = marker.getAttribute("data-list-base");
        var storeKey = "sort:" + base;
        var params = new URLSearchParams(window.location.search);
        if (params.get("sort")) {
            try {
                localStorage.setItem(
                    storeKey,
                    JSON.stringify({ sort: params.get("sort"), dir: params.get("dir") || "asc" })
                );
            } catch (e) {}
            return;
        }
        var saved = null;
        try {
            saved = JSON.parse(localStorage.getItem(storeKey));
        } catch (e) {}
        if (saved && saved.sort) {
            params.set("sort", saved.sort);
            if (saved.dir) {
                params.set("dir", saved.dir);
            }
            window.location.replace(base + "?" + params.toString());
        }
    })();

    // Styled confirm dialog (native <dialog>, CSP-safe) replacing window.confirm.
    // Returns a Promise<boolean>. Message is set via textContent (no injection).
    function showConfirm(message) {
        return new Promise(function (resolve) {
            var dlg = document.getElementById("app-confirm-dialog");
            if (!dlg) {
                dlg = document.createElement("dialog");
                dlg.id = "app-confirm-dialog";
                dlg.className = "confirm-dialog";
                dlg.innerHTML =
                    '<p class="confirm-message"></p>' +
                    '<div class="confirm-actions">' +
                    '<button type="button" class="button" data-variant="secondary" data-confirm-cancel>Cancel</button>' +
                    '<button type="button" class="button" data-variant="danger" data-confirm-ok>Confirm</button>' +
                    "</div>";
                document.body.appendChild(dlg);
            }
            dlg.querySelector(".confirm-message").textContent = message;
            function finish(result) {
                dlg.close();
                resolve(result);
            }
            dlg.querySelector("[data-confirm-cancel]").onclick = function () {
                finish(false);
            };
            dlg.querySelector("[data-confirm-ok]").onclick = function () {
                finish(true);
            };
            // Esc / backdrop dismissal counts as cancel.
            dlg.oncancel = function () {
                finish(false);
            };
            dlg.showModal();
        });
    }

    // Delete buttons: uses data-delete-url and data-delete-redirect attributes
    document.querySelectorAll("[data-delete-url]").forEach(function (button) {
        button.addEventListener("click", function (event) {
            event.preventDefault();
            var deleteUrl = this.getAttribute("data-delete-url");
            var redirectTo = this.getAttribute("data-delete-redirect");
            showConfirm("Are you sure you want to delete this item?").then(function (ok) {
                if (!ok) {
                    return;
                }
                var xhr = new XMLHttpRequest();
                xhr.open("DELETE", deleteUrl, true);
                xhr.onreadystatechange = function () {
                    if (xhr.readyState !== 4) {
                        return;
                    }
                    if (xhr.status === 200 || xhr.status === 204) {
                        window.location.href = redirectTo;
                    } else {
                        // Surface failures instead of silently doing nothing.
                        alert(
                            "Could not delete this item (error " +
                                xhr.status +
                                "). Please try again."
                        );
                    }
                };
                xhr.send();
            });
        });
    });

    // Confirm before submitting a form that carries data-confirm.
    document.querySelectorAll("form[data-confirm]").forEach(function (form) {
        var confirmed = false;
        form.addEventListener("submit", function (event) {
            if (confirmed) {
                return; // second pass after the user confirmed
            }
            event.preventDefault();
            showConfirm(form.getAttribute("data-confirm")).then(function (ok) {
                if (ok) {
                    confirmed = true;
                    form.submit();
                }
            });
        });
    });

    // Dynamic input cloning for "add more" buttons
    document.querySelectorAll(".add-more").forEach(function (button) {
        button.addEventListener("click", function () {
            var group = this.getAttribute("data-group");
            var first = document.getElementById(group + "-inputs").querySelector("input");
            if (first) {
                var clonedInput = first.cloneNode();
                clonedInput.value = "";
                var container = document.getElementById(group + "-inputs");
                container.appendChild(clonedInput);
            }
        });
    });

    // Clickable rows/cards: navigates to data-href on click. Clicks that land on
    // a nested interactive element (link, button, form control) are ignored so
    // those controls keep working; rows are also keyboard-activatable.
    document.querySelectorAll("[data-href]").forEach(function (el) {
        function navigateUnlessNested(event) {
            if (event.target.closest("a, button, input, select, textarea, label")) {
                return;
            }
            event.preventDefault();
            window.location.href = el.getAttribute("data-href");
        }
        el.addEventListener("click", navigateUnlessNested);
        // Make the row reachable and operable by keyboard.
        if (!el.hasAttribute("tabindex")) {
            el.setAttribute("tabindex", "0");
        }
        if (!el.hasAttribute("role")) {
            el.setAttribute("role", "link");
        }
        el.addEventListener("keydown", function (event) {
            if (event.key === "Enter") {
                navigateUnlessNested(event);
            }
        });
    });

    // Copy to clipboard: uses data-copy attribute
    document.querySelectorAll("[data-copy]").forEach(function (button) {
        button.addEventListener("click", function () {
            var text = this.getAttribute("data-copy");
            var btn = this;
            var original = btn.textContent;
            navigator.clipboard.writeText(text).then(function () {
                btn.textContent = "Copied!";
                window.setTimeout(function () {
                    btn.textContent = original;
                }, 1500);
            });
        });
    });

    // Select on focus: uses data-select-on-focus attribute
    document.querySelectorAll("[data-select-on-focus]").forEach(function (input) {
        input.addEventListener("focus", function () {
            this.select();
        });
    });

    // Auto-submit form on change: uses data-submit-on-change attribute
    document.querySelectorAll("[data-submit-on-change]").forEach(function (el) {
        el.addEventListener("change", function () {
            this.form.submit();
        });
    });

    // Mobile menu is now a CSS-only checkbox toggle (works without JS, so it
    // functions on guest pages that don't load app.js) — no handler needed.

    // Live job status: while a run is active (data-poll-status in the active
    // set), poll the page and refresh ONCE when the status changes — instead of
    // a full-page <meta refresh> every few seconds. No JSON endpoint needed.
    var pollMarker = document.querySelector("[data-poll-status]");
    if (pollMarker) {
        var status = pollMarker.getAttribute("data-poll-status");
        var active = status === "active" || status === "queued" || status === "running";
        if (active) {
            var pollUrl = window.location.href;
            var poll = window.setInterval(function () {
                fetch(pollUrl, { headers: { "X-Requested-With": "poll" } })
                    .then(function (r) {
                        return r.ok ? r.text() : null;
                    })
                    .then(function (html) {
                        if (!html) {
                            return;
                        }
                        var m = html.match(/data-poll-status="([^"]+)"/);
                        if (m && m[1] !== status) {
                            window.clearInterval(poll);
                            window.location.reload();
                        }
                    })
                    .catch(function () {});
            }, 4000);
        }
    }

    // Session refresh: only runs when body has data-authenticated
    if (document.body.hasAttribute("data-authenticated")) {
        setInterval(function () {
            fetch("/api/auth/oidc/refresh").then(function (r) {
                if (!r.ok) {
                    document.body.innerHTML =
                        '<div class="text-center mt-6">' +
                        "<h2>Session expired</h2><p>You have been logged out.</p>" +
                        '<a href="/api/auth/oidc/authorize">Sign in again</a></div>';
                }
            });
        }, 12 * 60 * 1000);
    }
});
