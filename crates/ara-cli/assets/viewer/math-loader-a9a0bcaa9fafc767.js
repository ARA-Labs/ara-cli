/* Panel math bridge. Vendor URLs follow this script, not the hub document base. */
(() => {
  "use strict";
  const scriptUrl = document.currentScript.src;
  const vendorUrl = new URL("./vendor/katex-0.19.0/", scriptUrl);
  const version = "0.19.0";
  let rendererPromise;
  let availabilityFailure;
  const fontLoads = new Map();

  function unavailable(detail) {
    const error = new Error(detail);
    error.name = "MathAvailabilityError";
    return error;
  }

  function resource(element, label) {
    return new Promise((resolve, reject) => {
      element.onload = () => {
        element.onload = element.onerror = null;
        resolve(element);
      };
      element.onerror = () => {
        element.onload = element.onerror = null;
        reject(unavailable(`Could not load local ${label}; reload to retry.`));
      };
      document.head.append(element);
    });
  }

  function loadRenderer() {
    if (!rendererPromise) {
      const script = document.createElement("script");
      script.src = new URL("katex.min.js", vendorUrl).href;
      const stylesheet = document.createElement("link");
      stylesheet.rel = "stylesheet";
      stylesheet.href = new URL("katex.min.css", vendorUrl).href;
      rendererPromise = Promise.all([
        resource(script, "math renderer"),
        resource(stylesheet, "math stylesheet"),
      ]).then(() => {
        if (globalThis.katex?.version !== version ||
            typeof globalThis.katex.render !== "function" || typeof globalThis.katex.__parse !== "function") {
          throw unavailable("The local math renderer has an unexpected version or API; reload to retry.");
        }
        const rules = Array.from(stylesheet.sheet?.cssRules ?? []);
        if (!rules.some(rule => rule.style?.fontFamily?.replaceAll('"', "") === "KaTeX_Main") ||
            !rules.some(rule => rule.selectorText === ".katex")) {
          throw unavailable("The local math stylesheet is missing its required rules; reload to retry.");
        }
        if (!document.fonts?.load) {
          throw unavailable("This browser cannot verify the local math fonts.");
        }
        return globalThis.katex;
      }).catch(error => {
        availabilityFailure = error.name === "MathAvailabilityError"
          ? error : unavailable("The local math assets are unavailable; reload to retry.");
        throw availabilityFailure;
      });
    }
    return rendererPromise;
  }

  function fontWeight(value) {
    return value === "normal" ? "400" : value === "bold" ? "700" : value;
  }

  function requireFont(family, style, weight, text) {
    const key = `${style} ${weight} 16px "${family}"`;
    if (!fontLoads.has(key)) {
      fontLoads.set(key, document.fonts.load(key, text).then(faces => {
        if (!faces.length || !faces.some(face =>
          face.family.replaceAll('"', "") === family && face.status === "loaded" &&
          face.style === style && fontWeight(face.weight) === fontWeight(weight)) ||
          !document.fonts.check(key, text)) {
          throw unavailable(`The required local ${family} font did not load; reload to retry.`);
        }
      }).catch(() => {
        availabilityFailure = unavailable(`The required local ${family} font did not load; reload to retry.`);
        throw availabilityFailure;
      }));
    }
    return fontLoads.get(key);
  }

  function verifyFonts(probe) {
    const required = [requireFont("KaTeX_Main", "normal", "400", "M")];
    for (const element of probe.querySelectorAll(".katex-html *")) {
      const text = Array.from(element.childNodes)
        .filter(node => node.nodeType === Node.TEXT_NODE)
        .map(node => node.textContent).join("");
      if (!text.trim()) continue;
      const computed = getComputedStyle(element);
      const family = computed.fontFamily.split(",")[0].trim().replaceAll('"', "");
      if (family.startsWith("KaTeX_")) {
        required.push(requireFont(family, computed.fontStyle, computed.fontWeight, text));
      }
    }
    return Promise.all(required);
  }

  // KaTeX renders denied commands instead of throwing. Validate its pinned
  // parse tree: conditional HTML/MathML branches can discard error nodes from
  // both outputs. This marker cannot be an author color (Parser.parseColorGroup).
  const rejectedColor = "var(--ara-math-denied-command)";
  function hasDeniedCommand(tree) {
    const pending = [...tree];
    while (pending.length) {
      const node = pending.pop();
      if (!node || typeof node !== "object") continue;
      if (node.type === "color" && node.color === rejectedColor) return true;
      for (const [key, value] of Object.entries(node)) {
        if (key === "loc" || !value || typeof value !== "object") continue;
        if (Array.isArray(value)) pending.push(...value);
        else pending.push(value);
      }
    }
    return false;
  }

  async function prepare(job, tex, display) {
    let probe;
    try {
      const renderer = await loadRenderer();
      if (!job.live) return { status: "cancelled", detail: "" };
      if (availabilityFailure) throw availabilityFailure;
      const host = document.createElement("span");
      const options = {
        displayMode: display,
        output: "htmlAndMathml",
        trust: false,
        globalGroup: false,
        maxExpand: 1000,
        maxSize: 20,
        throwOnError: true,
        errorColor: rejectedColor,
      };
      // Keep validation macros separate: \gdef must not seed the rendering pass.
      if (hasDeniedCommand(renderer.__parse(tex, { ...options, macros: {} }))) {
        throw new Error("Trust-sensitive math commands are not permitted.");
      }
      renderer.render(tex, host, { ...options, macros: {} });
      // The temporary probe belongs only to this job, never to a Leptos host.
      // CSS must apply before we can identify the font faces actually used.
      probe = document.createElement("div");
      probe.setAttribute("aria-hidden", "true");
      probe.style.cssText = "position:fixed;left:0;top:0;visibility:hidden;pointer-events:none;max-width:100vw;overflow:hidden;white-space:normal;";
      probe.append(host);
      job.probe = probe;
      document.body.append(probe);
      await verifyFonts(probe);
      if (!job.live) return { status: "cancelled", detail: "" };
      if (availabilityFailure) throw availabilityFailure;
      host.remove();
      return { status: "ready", detail: "", host };
    } catch (error) {
      if (!job.live) return { status: "cancelled", detail: "" };
      return {
        status: error.name === "MathAvailabilityError" ? "unavailable" : "equation",
        detail: String(error.message ?? error),
      };
    } finally {
      probe?.remove();
      job.probe = null;
    }
  }

  globalThis.AraMath = {
    createJob(tex, display) {
      const job = { live: true, probe: null };
      job.promise = prepare(job, tex, display);
      return job;
    },
    jobPromise(job) { return job.promise; },
    cancelJob(job) {
      job.live = false;
      job.probe?.remove();
      job.probe = null;
    },
  };
})();
