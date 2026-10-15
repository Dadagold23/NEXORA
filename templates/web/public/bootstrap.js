// PROJECT_NAME - the bridge between the NXL program and the page.
//
// The NXL program talks to the outside world through print(), which the
// JavaScript backend compiles to console.log. This file runs before /app.js,
// takes over console.log, and turns the boot log into the landing page:
//
//   booting <name>   -> the hero heading
//   tagline <text>   -> the hero sub-heading
//   route  <path>    -> the served-routes list
//   feature  <text>  -> the feature list
//   READY            -> clear the splash screen and mark the app online
//
// Nothing here is NXL-specific beyond those lines, so a page can ignore this
// file and still work: everything degrades to a readable boot log.

(function () {
  "use strict";

  var logElement = document.getElementById("log");
  var splash = document.getElementById("splash");
  var splashStage = document.getElementById("splash-stage");
  var page = document.getElementById("page");
  var status = document.getElementById("status");
  var statusText = document.getElementById("status-text");
  var features = document.getElementById("features");
  var routes = document.getElementById("routes");
  var appName = document.getElementById("app-name");
  var appTagline = document.getElementById("app-tagline");

  var originalLog = console.log.bind(console);
  var ready = false;

  // The server already rendered a list for each of these, from the same
  // modules. The program's own report replaces those items rather than being
  // appended to them, so the page does not show everything twice.
  function clearGenerated(list) {
    var placeholder = list.querySelector(".placeholder");
    if (placeholder) {
      list.removeChild(placeholder);
    }
    var remaining = [];
    for (var i = 0; i < list.children.length; i++) {
      if (list.children[i].className === "generated") {
        remaining.push(list.children[i]);
      } else {
        list.removeChild(list.children[i]);
      }
    }
    return remaining;
  }

  function resetList(list) {
    clearGenerated(list);
    for (var i = 0; i < list.children.length; i++) {
      list.removeChild(list.children[i]);
    }
  }

  function addItem(list, text) {
    clearGenerated(list);
    var item = document.createElement("li");
    item.textContent = text;
    item.className = "generated";
    list.appendChild(item);
  }

  function appendLog(text, isReady) {
    if (!logElement) {
      return;
    }
    var line = document.createElement("span");
    line.textContent = text + "\n";
    if (isReady) {
      line.className = "is-ready";
    }
    logElement.appendChild(line);
    logElement.scrollTop = logElement.scrollHeight;
  }

  function showPage() {
    if (ready) {
      return;
    }
    ready = true;
    if (splash) {
      splash.classList.add("is-done");
    }
    if (page) {
      page.classList.add("is-live");
    }
    if (status) {
      status.classList.add("is-live");
    }
    if (statusText) {
      statusText.textContent = "app ready";
    }
  }

  function handle(line) {
    var text = String(line);
    appendLog(text, text === "READY");

    if (text.indexOf("booting ") === 0) {
      var name = text.slice("booting ".length);
      if (appName) {
        appName.textContent = name;
      }
      document.title = name;
      // Start from a clean page: the server-rendered lists describe what the
      // app is about to report, and are replaced as it reports.
      resetList(features);
      resetList(routes);
    } else if (text.indexOf("tagline ") === 0) {
      if (appTagline) {
        appTagline.textContent = text.slice("tagline ".length);
      }
    } else if (text.indexOf("route ") === 0) {
      addItem(routes, text.slice("route ".length).trim());
    } else if (text.indexOf("feature ") === 0) {
      addItem(features, text.slice("feature ".length).trim());
    } else if (splashStage && text.indexOf("booting ") === 0) {
      splashStage.textContent = "starting";
    }

    if (text === "READY") {
      showPage();
    }
  }

  console.log = function () {
    var parts = [];
    for (var i = 0; i < arguments.length; i++) {
      parts.push(typeof arguments[i] === "string" ? arguments[i] : String(arguments[i]));
    }
    handle(parts.join(" "));
  };

  // A program that throws never prints READY. Surface the reason instead of
  // leaving the splash screen up with no explanation.
  window.addEventListener("error", function (event) {
    appendLog("error: " + (event.message || "the app failed to run"), false);
    if (splashStage) {
      splashStage.textContent = "failed to start";
    }
    if (statusText) {
      statusText.textContent = "the app did not finish starting";
    }
  });

  // If the bundle never loads at all, there is no program output to wait for.
  window.addEventListener("load", function () {
    setTimeout(function () {
      if (!ready) {
        if (splashStage) {
          splashStage.textContent = "still starting";
        }
      }
    }, 4000);
  });

  window.__nxlApp = { handle: handle, showPage: showPage, log: originalLog };
})();