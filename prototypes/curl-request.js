/* Local cURL text conversion. No shell execution, file reads, or network calls. */
(function (root) {
  "use strict";

  function words(source) {
    const result = [];
    let word = "", quote = null, started = false;
    const flush = () => { if (started) result.push(word); word = ""; started = false; };
    for (let i = 0; i < source.length; i++) {
      const ch = source[i];
      if (quote === "'") {
        if (ch === "'") quote = null; else word += ch;
        continue;
      }
      if (ch === "\\") {
        if (i + 1 === source.length) throw new Error("Complete the trailing backslash or line continuation.");
        const next = source[++i];
        if (next === "\n") continue;
        if (next === "\r" && source[i + 1] === "\n") { i++; continue; }
        if (quote === '"' && !['"', "\\", "$", "`"].includes(next)) word += "\\";
        word += next; started = true; continue;
      }
      if (ch === "$" || ch === "`") throw new Error("Paste resolved values instead of shell variables, substitutions, or special shell quoting.");
      if (quote === '"') {
        if (ch === '"') quote = null; else word += ch;
        continue;
      }
      if (ch === "'" || ch === '"') { quote = ch; started = true; continue; }
      if (/\s/.test(ch)) { flush(); continue; }
      if (/[|;&<>]/.test(ch)) throw new Error("Paste a single cURL command. Quote URLs or values containing shell punctuation.");
      if (ch === "\0") throw new Error("The command contains a null character.");
      word += ch; started = true;
    }
    if (quote) throw new Error("Close the quotation mark in your cURL command.");
    flush();
    return result;
  }

  const valueFlags = new Set(["--request", "--url", "--header", "--data", "--data-raw", "--data-binary", "--data-urlencode", "--json", "--user", "--oauth2-bearer", "--user-agent", "--referer", "--cookie"]);
  const switches = new Set(["--get", "--head", "--location", "--compressed", "--insecure", "--silent", "--show-error", "--verbose", "--include", "--no-progress-meter", "--globoff"]);
  const shortFlags = { X:"--request", H:"--header", d:"--data", u:"--user", A:"--user-agent", e:"--referer", b:"--cookie", G:"--get", I:"--head", L:"--location", k:"--insecure", s:"--silent", S:"--show-error", v:"--verbose", i:"--include", g:"--globoff" };
  const persistentFlags = new Set(["--location", "--compressed", "--insecure", "--globoff"]);
  const optionLabels = { "--location":"Follow redirects", "--compressed":"Decode compressed responses", "--insecure":"Skip TLS certificate verification", "--globoff":"Disable URL globbing" };
  const encode = value => encodeURIComponent(value).replace(/[!'()*]/g, ch => "%" + ch.charCodeAt(0).toString(16).toUpperCase());

  function parse(source) {
    if (source.includes("\0")) throw new Error("The command contains a null character.");
    const args = words(source.trim());
    if (!/^(?:.*[\\/])?curl(?:\.exe)?$/i.test(args.shift() || "")) throw new Error("Start with a cURL command, such as curl 'https://api.example.com/users'.");
    let urlText = null, explicitMethod = null, get = false, head = false, json = false, basic = null, bearer = null;
    const headers = [], chunks = [], curlOptions = [];
    const setUrl = value => {
      if (urlText !== null) throw new Error("Import one URL at a time.");
      urlText = value;
    };
    const addHeader = (key, value, forceEmpty = false) => {
      if (!/^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/.test(key) || /[\r\n]/.test(value)) throw new Error("Use a valid header name and a single-line value.");
      headers.push({enabled:true, key, value, note:"Imported from cURL", forceEmpty});
    };
    const apply = (flag, value) => {
      if (persistentFlags.has(flag)) { if (!curlOptions.includes(flag)) curlOptions.push(flag); return; }
      if (flag === "--url") return setUrl(value);
      if (flag === "--request") { explicitMethod = value; return; }
      if (flag === "--get") { get = true; return; }
      if (flag === "--head") { head = true; return; }
      if (flag === "--user") { basic = value; return; }
      if (flag === "--oauth2-bearer") { bearer = value; return; }
      if (flag === "--user-agent") return addHeader("User-Agent", value);
      if (flag === "--referer") return addHeader("Referer", value);
      if (flag === "--cookie") {
        if (!value.includes("=")) throw new Error("Cookie files are not supported. Paste cookies as name=value.");
        return addHeader("Cookie", value);
      }
      if (flag === "--header") {
        if (value.startsWith("@")) throw new Error("Header files are not supported. Paste each header with -H.");
        const colon = value.indexOf(":");
        if (colon < 0 && value.endsWith(";")) return addHeader(value.slice(0,-1), "", true);
        if (colon < 1) throw new Error("Write headers as 'Name: value'.");
        return addHeader(value.slice(0,colon).trim(), value.slice(colon + 1).trim());
      }
      if (flag.startsWith("--data") || flag === "--json") {
        if (flag !== "--data-raw" && value.startsWith("@")) throw new Error("File-based bodies are not supported. Paste the body inline with --data-raw.");
        if (flag === "--data-urlencode") {
          const equal = value.indexOf("=");
          if (equal < 0 && value.includes("@")) throw new Error("File-based form fields are not supported. Paste name=value instead.");
          value = equal < 0 ? encode(value) : (equal === 0 ? "" : value.slice(0,equal + 1)) + encode(value.slice(equal + 1));
        }
        if (chunks.length && (flag === "--json") !== json) throw new Error("Use either --json or --data options for this request, not both.");
        json = flag === "--json"; chunks.push(value);
      }
    };
    const takeValue = (flag, index) => {
      if (index >= args.length) throw new Error("Add a value after " + flag + ".");
      return args[index];
    };
    for (let i = 0; i < args.length; i++) {
      const arg = args[i];
      if (arg === "--") { for (const url of args.slice(i + 1)) setUrl(url); break; }
      if (arg.startsWith("--")) {
        const eq = arg.indexOf("="), flag = eq < 0 ? arg : arg.slice(0,eq);
        if (!valueFlags.has(flag) && !switches.has(flag)) throw new Error("Unsupported option: " + flag + ". This importer supports inline HTTP requests.");
        if (switches.has(flag)) {
          if (eq >= 0) throw new Error(flag + " does not take a value.");
          apply(flag); continue;
        }
        apply(flag, eq < 0 ? takeValue(flag, ++i) : arg.slice(eq + 1));
      } else if (arg.startsWith("-")) {
        for (let at = 1; at < arg.length; at++) {
          const flag = shortFlags[arg[at]];
          if (!flag) throw new Error("Unsupported option: -" + arg[at] + ".");
          if (valueFlags.has(flag)) { apply(flag, at + 1 < arg.length ? arg.slice(at + 1) : takeValue(flag, ++i)); break; }
          apply(flag);
        }
        if (arg === "-") throw new Error("Paste a URL or a supported cURL option.");
      } else setUrl(arg);
    }
    if (!urlText) throw new Error("Add an HTTP or HTTPS URL to the command.");
    let url;
    try { url = new URL(urlText); } catch (_) { throw new Error("Use a complete URL beginning with http:// or https://."); }
    if (!["http:","https:"].includes(url.protocol)) throw new Error("Only HTTP and HTTPS requests can be imported.");
    if (head && chunks.length && !get) throw new Error("A --head request cannot also use --data. Remove one of these options.");
    const method = explicitMethod ?? (head ? "HEAD" : get ? "GET" : chunks.length ? "POST" : "GET");
    if (!/^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/.test(method)) throw new Error("Use a valid HTTP method after --request.");
    const hasHeader = name => headers.some(h => h.key.toLowerCase() === name.toLowerCase());
    if (basic !== null && !hasHeader("Authorization")) {
      if (!basic.includes(":")) throw new Error("Provide --user as username:password; interactive password prompts are not supported.");
      addHeader("Authorization", "Basic " + btoa(Array.from(new TextEncoder().encode(basic), b => String.fromCharCode(b)).join("")));
    }
    if (bearer !== null && !hasHeader("Authorization")) addHeader("Authorization", "Bearer " + bearer);
    let body = chunks.join(json ? "" : "&");
    if (get && chunks.length) { url.search += (url.search ? "&" : "?") + body; body = ""; }
    const hasBody = chunks.length > 0 && !get;
    if (hasBody && !hasHeader("Content-Type")) addHeader("Content-Type", json ? "application/json" : "application/x-www-form-urlencoded");
    if (json && !hasHeader("Accept")) addHeader("Accept", "application/json");
    let bodyFormat = hasHeader("Content-Type") && headers.some(h => /json/i.test(h.value) && h.key.toLowerCase() === "content-type") ? "json" : "raw";
    try { if (hasBody) { JSON.parse(body); bodyFormat = "json"; } } catch (_) { /* Preserve non-JSON bodies verbatim. */ }
    let authKind = "none", token = "";
    const auth = headers.filter(h => h.key.toLowerCase() === "authorization");
    if (auth.length === 1 && /^Bearer\s+\S/i.test(auth[0].value)) {
      authKind = "bearer"; token = auth[0].value.replace(/^Bearer\s+/i, ""); headers.splice(headers.indexOf(auth[0]),1);
    }
    return {method, url:url.href, headers, body, hasBody, bodyFormat, authKind, token, curlOptions};
  }

  function stringify(request) {
    const quote = value => "'" + String(value).replace(/'/g, "'\\''") + "'";
    const lines = ["curl --request " + quote(request.method), "  --url " + quote(request.url)];
    for (const flag of request.curlOptions || []) if (persistentFlags.has(flag)) lines.push("  " + flag);
    const headers = request.headers.filter(h => h.enabled && h.key);
    for (const h of headers) lines.push("  --header " + quote(h.key + (h.value ? ": " + h.value : h.forceEmpty ? ";" : ":")));
    if (request.authKind === "bearer" && request.token && !headers.some(h => h.key.toLowerCase() === "authorization")) lines.push("  --header " + quote("Authorization: Bearer " + request.token));
    if (request.hasBody) {
      if (request.bodyFormat === 'multipart') {
        // cURL's form grammar has its own quoting, inside shell quoting.
        // https://curl.se/docs/manpage.html#-F
        const filePath = name => '"./' + name.replace(/["\\]/g, '\\$&') + '"';
        for (const part of request.multipart || []) {
          if (part.type === 'file') {
            if (!part.file) throw new Error('Choose a file for ' + part.key + '.');
            const mime = /^[\w.+-]+\/[\w.+-]+$/.test(part.file.type || '') ? ';type=' + part.file.type : '';
            lines.push('  --form ' + quote(part.key + '=@' + filePath(part.file.name) + mime));
          } else lines.push('  --form-string ' + quote(part.key + '=' + part.value));
        }
      } else if (request.bodyFormat === 'binary') {
        if (!request.binary) throw new Error('Choose a file for the binary body.');
        lines.push('  --data-binary ' + quote('@./' + request.binary.name));
      } else lines.push("  --data-raw " + quote(request.body));
    }
    return lines.join(" \\\n");
  }

  const api = {parse, stringify, optionLabels};
  if (typeof module !== "undefined" && module.exports) module.exports = api;
  else root.CurlRequest = api;
})(globalThis);
