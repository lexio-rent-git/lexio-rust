"""Generate src/resources.rs from spec/openapi.json (Lexio Rent API v1).

Usage: python3 scripts/generate.py
Each OpenAPI operation becomes an async method, grouped by resource (first path
segment): GET /vehicles -> client.vehicles().list(&params),
POST /rentals/{rentalId}/cancel -> client.rentals().cancel(rental_id, &body, None).
"""
from __future__ import annotations

import json
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parent.parent
SPEC = json.loads((ROOT / "spec" / "openapi.json").read_text(encoding="utf-8"))
OUT = ROOT / "src" / "resources.rs"
METHODS = ("get", "post", "put", "patch", "delete")
RUST_KEYWORDS = {
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for", "if", "impl", "in",
    "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super",
    "trait", "true", "type", "unsafe", "use", "where", "while", "async", "await", "dyn", "abstract", "become", "box",
    "do", "final", "macro", "override", "priv", "typeof", "unsized", "virtual", "yield", "try", "gen",
}


def snake(name: str) -> str:
    return re.sub(r"(?<=[a-z0-9])([A-Z])", r"_\1", name).replace("-", "_").lower()


def safe(name: str) -> str:
    return name + "_" if name in RUST_KEYWORDS else name


def deref(obj):
    if isinstance(obj, dict) and "$ref" in obj:
        node = SPEC
        for part in obj["$ref"].lstrip("#/").split("/"):
            node = node[part]
        return node
    return obj


def singular(tok: str) -> str:
    if tok.endswith("ies"):
        return tok[:-3] + "y"
    return tok[:-1] if tok.endswith("s") and not tok.endswith("ss") else tok


def method_name(op_id: str, resource: str) -> str:
    toks = snake(op_id).split("_")
    res = resource.split("-")
    for v in (res, [singular(t) for t in res], res[:-1] + [singular(res[-1])]):
        n = len(v)
        for i in range(1, len(toks) - n + 1):
            if toks[i : i + n] == v:
                return safe("_".join(toks[:i] + toks[i + n :]))
    return safe("_".join(toks))


def type_name(resource: str) -> str:
    return "".join(p.capitalize() for p in resource.split("-"))


def doc_lines(text: str, indent: str) -> list[str]:
    text = " ".join((text or "").split())
    text = text.replace("<", "&lt;").replace(">", "&gt;")
    return [f"{indent}/// {text}"] if text else []


resources: dict[str, list[str]] = {}
seen: dict[str, set[str]] = {}
count = 0
for path, ops in SPEC["paths"].items():
    resource = path.strip("/").split("/")[0]
    path_level = [deref(p) for p in ops.get("parameters", [])]
    for http, op in ops.items():
        if http not in METHODS:
            continue
        name = method_name(op["operationId"], resource)
        if name in seen.setdefault(resource, set()):
            raise SystemExit(f"collision {resource}.{name}")
        seen[resource].add(name)
        params = path_level + [deref(p) for p in op.get("parameters", [])]
        path_params = [p for p in params if p["in"] == "path"]
        query_params = [p for p in params if p["in"] == "query"]
        idem = next((p for p in params if p["in"] == "header" and p["name"] == "Idempotency-Key"), None)
        has_body = "requestBody" in op
        body_required = has_body and op["requestBody"].get("required", False)

        args, call_body = [], "None"
        for p in path_params:
            args.append(f"{safe(snake(p['name']))}: &str")
        if has_body:
            if body_required:
                args.append("body: &Value")
                call_body = "Some(body)"
            else:
                args.append("body: Option<&Value>")
                call_body = "body"
        call_params = "None"
        if query_params:
            args.append("params: &Params")
            call_params = "Some(params)"
        call_idem = "None"
        if idem is not None:
            args.append("idempotency_key: Option<&str>")
            call_idem = "idempotency_key"
        if path_params:
            fmt = re.sub(r"\{\w+\}", "{}", path)
            path_expr = f'&format!("{fmt}", ' + ", ".join(f"seg({safe(snake(p['name']))})" for p in path_params) + ")"
        else:
            path_expr = f'"{path}"'

        lines = []
        lines += doc_lines((op.get("summary") or op["operationId"]).rstrip(".") + ".", "    ")
        lines.append("    ///")
        lines.append(f"    /// `{http.upper()} {path}` (operationId `{op['operationId']}`).")
        if query_params:
            lines.append("    ///")
            lines.append("    /// Query parameters: " + ", ".join(f"`{p['name']}`" for p in query_params) + ".")
        if idem is not None:
            lines.append("    ///")
            req = "required by the API, generated when `None`" if idem.get("required") else "optional"
            lines.append(f"    /// `Idempotency-Key`: {req}; reused on retries.")
        sig = ", ".join(["&self"] + args)
        lines.append(f"    pub async fn {name}({sig}) -> Result<Value> {{")
        lines.append(
            f"        self.client.request(Method::{http.upper()}, {path_expr}, {call_params}, {call_body}, {call_idem}, {str(bool(idem and idem.get('required'))).lower()}).await"
        )
        lines.append("    }")
        if http == "get" and any(p["name"] == "cursor" for p in query_params):
            lines.append("")
            lines.append(f"    /// Every item of [`Self::{name}`], following `nextCursor` (at most `max_items` if set).")
            all_args = ", ".join(["&self"] + [a for a in args if a != "params: &Params"] + ["params: &Params", "max_items: Option<usize>"])
            lines.append(f"    pub async fn {name}_all({all_args}) -> Result<Vec<Value>> {{")
            lines.append(f"        self.client.list_all({path_expr}, params, max_items).await")
            lines.append("    }")
        resources.setdefault(resource, []).append("\n".join(lines))
        count += 1

out = [
    "//! Resources of the Lexio Rent API. GENERATED by scripts/generate.py from spec/openapi.json: do not edit.",
    "#![allow(clippy::too_many_arguments)]",
    "",
    "use reqwest::Method;",
    "use serde_json::Value;",
    "",
    "use crate::client::{seg, Lexio};",
    "use crate::error::Result;",
    "use crate::params::Params;",
    "",
    f'/// Version of the OpenAPI document the resources were generated from.',
    f'pub const SPEC_VERSION: &str = "{SPEC["info"]["version"]}";',
    "",
    f"/// Number of API operations covered.",
    f"pub const OPERATION_COUNT: usize = {count};",
]
accessors = []
for resource, methods in resources.items():
    t = type_name(resource)
    attr = safe(snake(resource))
    out += [
        "",
        f"/// Operations on `/{resource}`. Get it with [`Lexio::{attr}`].",
        "#[derive(Debug, Clone, Copy)]",
        f"pub struct {t}<'a> {{",
        "    client: &'a Lexio,",
        "}",
        "",
        f"impl<'a> {t}<'a> {{",
        "\n\n".join(methods),
        "}",
    ]
    accessors.append(f"    /// Operations on `/{resource}`.\n    pub fn {attr}(&self) -> {t}<'_> {{\n        {t} {{ client: self }}\n    }}")
out += ["", "impl Lexio {", "\n\n".join(accessors), "}", ""]
OUT.write_text("\n".join(out), encoding="utf-8")
print(f"src/resources.rs: {count} operations, {len(resources)} resources")
for r in resources:
    print(" ", safe(snake(r)), sorted(seen[r]))
