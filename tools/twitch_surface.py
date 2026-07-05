#!/usr/bin/env python3
from __future__ import annotations

import argparse
import collections
import dataclasses
import datetime as dt
import html
import json
import pathlib
import re
import sys
from typing import Any


SOURCE_URLS = {
    "helix_reference": "https://dev.twitch.tv/docs/api/reference",
    "eventsub_types": "https://dev.twitch.tv/docs/eventsub/eventsub-subscription-types/",
    "oauth_tokens": "https://dev.twitch.tv/docs/authentication/getting-tokens-oauth",
    "oauth_oidc": "https://dev.twitch.tv/docs/authentication/getting-tokens-oidc",
    "oauth_validate": "https://dev.twitch.tv/docs/authentication/validate-tokens",
    "oauth_revoke": "https://dev.twitch.tv/docs/authentication/revoke-tokens",
    "eventsub_webhooks": "https://dev.twitch.tv/docs/eventsub/handling-webhook-events/",
    "changelog": "https://dev.twitch.tv/docs/change-log",
    "product_lifecycle": "https://dev.twitch.tv/docs/product-lifecycle",
}


AUTH_SURFACE = {
    "generated_from": "manual",
    "flows": [
        {
            "id": "oauth_implicit",
            "name": "OAuth implicit grant flow",
            "token_kind": "user",
            "authorize_endpoint": "https://id.twitch.tv/oauth2/authorize",
            "response_type": "token",
        },
        {
            "id": "oauth_client_credentials",
            "name": "OAuth client credentials grant flow",
            "token_kind": "app",
            "token_endpoint": "https://id.twitch.tv/oauth2/token",
            "grant_type": "client_credentials",
        },
        {
            "id": "oauth_authorization_code",
            "name": "OAuth authorization code grant flow",
            "token_kind": "user",
            "authorize_endpoint": "https://id.twitch.tv/oauth2/authorize",
            "token_endpoint": "https://id.twitch.tv/oauth2/token",
            "response_type": "code",
        },
        {
            "id": "oauth_device_code",
            "name": "OAuth device code grant flow",
            "token_kind": "user",
            "device_endpoint": "https://id.twitch.tv/oauth2/device",
            "token_endpoint": "https://id.twitch.tv/oauth2/token",
            "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
        },
        {
            "id": "oauth_refresh",
            "name": "OAuth refresh token flow",
            "token_kind": "user",
            "token_endpoint": "https://id.twitch.tv/oauth2/token",
            "grant_type": "refresh_token",
        },
        {
            "id": "oidc_implicit",
            "name": "OIDC implicit grant flow",
            "token_kind": "user",
            "authorize_endpoint": "https://id.twitch.tv/oauth2/authorize",
            "response_type": "token id_token",
        },
        {
            "id": "oidc_authorization_code",
            "name": "OIDC authorization code grant flow",
            "token_kind": "user",
            "authorize_endpoint": "https://id.twitch.tv/oauth2/authorize",
            "token_endpoint": "https://id.twitch.tv/oauth2/token",
            "response_type": "code",
        },
    ],
    "endpoints": [
        {
            "id": "oauth_authorize",
            "name": "OAuth authorize",
            "method": "GET",
            "url": "https://id.twitch.tv/oauth2/authorize",
        },
        {
            "id": "oauth_token",
            "name": "OAuth token",
            "method": "POST",
            "url": "https://id.twitch.tv/oauth2/token",
        },
        {
            "id": "oauth_device",
            "name": "OAuth device authorization",
            "method": "POST",
            "url": "https://id.twitch.tv/oauth2/device",
        },
        {
            "id": "oauth_validate",
            "name": "OAuth validate",
            "method": "GET",
            "url": "https://id.twitch.tv/oauth2/validate",
        },
        {
            "id": "oauth_revoke",
            "name": "OAuth revoke",
            "method": "POST",
            "url": "https://id.twitch.tv/oauth2/revoke",
        },
        {
            "id": "oidc_configuration",
            "name": "OIDC discovery document",
            "method": "GET",
            "url": "https://id.twitch.tv/oauth2/.well-known/openid-configuration",
        },
        {
            "id": "oidc_keys",
            "name": "OIDC JWKs",
            "method": "GET",
            "url": "https://id.twitch.tv/oauth2/keys",
        },
        {
            "id": "oidc_userinfo",
            "name": "OIDC userinfo",
            "method": "GET",
            "url": "https://id.twitch.tv/oauth2/userinfo",
        },
    ],
    "rules": {
        "validate_interval_seconds": 3600,
        "validate_required_for_oauth_sessions": True,
    },
}


def now_iso() -> str:
    return dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat()


def clean_html(raw: str) -> str:
    text = re.sub(r"<br\s*/?>", "\n", raw, flags=re.I)
    text = re.sub(r"</(p|div|li|ul|ol|tr|td|th|h2|h3|h4)>", "\n", text, flags=re.I)
    text = re.sub(r"<[^>]+>", "", text)
    text = html.unescape(text)
    lines = []
    for line in text.splitlines():
        line = re.sub(r"\s+", " ", line).strip()
        if line:
            lines.append(line)
    return "\n".join(lines)


def normalize_space(value: str) -> str:
    return re.sub(r"\s+", " ", value).strip()


def snake_case(value: str) -> str:
    value = value.replace("&", " and ")
    value = re.sub(r"[^A-Za-z0-9]+", "_", value)
    value = re.sub(r"_+", "_", value)
    return value.strip("_").lower()


def camel_case(value: str) -> str:
    return "".join(part.capitalize() for part in snake_case(value).split("_"))


def rust_string(value: str) -> str:
    return json.dumps(value, ensure_ascii=False)


def classify_stability(raw: str) -> str:
    lowered = raw.lower()
    if "pill-beta" in lowered or ">beta<" in lowered:
        return "beta"
    if "pill-new" in lowered or ">new<" in lowered:
        return "new"
    return "ga"


def extract_scopes(auth_text: str) -> list[str]:
    scopes = {
        match.group(0)
        for match in re.finditer(r"\b[a-z][a-z_]+(?::[a-z_]+){1,4}\b", auth_text)
    }
    return sorted(scopes)


def classify_auth(auth_text: str) -> dict[str, Any]:
    lowered = auth_text.lower()
    if "signed json web token" in lowered or "jwt" in lowered:
        kind = "extension_jwt"
    elif "app access token" in lowered and "user access token" in lowered:
        kind = "either"
    elif "app access token" in lowered:
        kind = "app"
    elif "user access token" in lowered:
        kind = "user"
    elif "authorization header" in lowered:
        kind = "custom"
    else:
        kind = "none"

    return {
        "kind": kind,
        "scopes": extract_scopes(auth_text),
        "raw": auth_text,
    }


def parse_section(section_html: str, heading: str) -> str:
    pattern = re.compile(
        rf"<h3[^>]*>\s*{re.escape(heading)}\s*</h3>(?P<body>.*?)(?=<h[23][^>]*>|$)",
        re.I | re.S,
    )
    match = pattern.search(section_html)
    return clean_html(match.group("body")) if match else ""


def parse_helix(reference_html: str) -> dict[str, Any]:
    summary_start = reference_html.find('<h1 id="twitch-api-reference">')
    if summary_start == -1:
        raise RuntimeError("failed to locate helix summary table")
    summary_html = reference_html[summary_start:]

    row_re = re.compile(
        r"<tr>\s*<td>(?P<group>.*?)</td>\s*<td><a href=\"#(?P<slug>[^\"]+)\">(?P<name>.*?)</a></td>\s*<td><p>(?P<description>.*?)</p>\s*</td>\s*</tr>",
        re.S,
    )
    section_re = re.compile(r"<h2 id=\"(?P<slug>[^\"]+)\">(?P<name>.*?)</h2>(?P<body>.*?)(?=<h2 id=\"|$)", re.S)
    sections = {
        match.group("slug"): match.group("body")
        for match in section_re.finditer(reference_html)
    }

    endpoints = []
    groups: dict[str, int] = collections.Counter()
    for match in row_re.finditer(summary_html):
        group = clean_html(match.group("group"))
        slug = match.group("slug").strip()
        name = clean_html(match.group("name"))
        description_html = match.group("description")
        description = clean_html(description_html)
        stability = classify_stability(description_html)
        body = sections.get(slug, "")
        method_match = re.search(
            r"<code class=\"highlighter-rouge\">(GET|POST|PUT|PATCH|DELETE)\s+https://api\.twitch\.tv/helix(?P<path>[^<]+)</code>",
            body,
            re.I,
        )
        method = method_match.group(1).upper() if method_match else "GET"
        path = method_match.group("path").strip() if method_match else f"/{slug}"
        auth_text = parse_section(body, "Authorization")
        supports_pagination = (
            "pagination" in body.lower()
            or " after " in body.lower()
            or " before " in body.lower()
            or "cursor" in body.lower()
        )
        endpoint_id = snake_case(f"{group}_{name}")
        endpoints.append(
            {
                "id": endpoint_id,
                "group": group,
                "name": name,
                "slug": slug,
                "description": description,
                "stability": stability,
                "method": method,
                "path": path,
                "url": f"https://api.twitch.tv/helix{path}",
                "auth": classify_auth(auth_text),
                "supports_pagination": supports_pagination,
            }
        )
        groups[group] += 1

    return {
        "generated_at": now_iso(),
        "source": SOURCE_URLS["helix_reference"],
        "group_counts": dict(sorted(groups.items())),
        "endpoints": endpoints,
    }


def parse_eventsub(eventsub_html: str) -> dict[str, Any]:
    start = eventsub_html.find('<h1 id="subscription-types">')
    if start == -1:
        raise RuntimeError("failed to locate EventSub subscription table")
    end = eventsub_html.find("## Public Beta Program")
    summary_html = eventsub_html[start:end if end != -1 else None]

    row_re = re.compile(
        r"<tr>\s*<td>(?P<name>.*?)</td>\s*<td><code[^>]*>(?P<subscription_type>[^<]+)</code></td>\s*<td><code[^>]*>(?P<version>[^<]+)</code></td>\s*<td>(?P<description>.*?)</td>\s*</tr>",
        re.S,
    )

    subscriptions = []
    for match in row_re.finditer(summary_html):
        name_html = match.group("name")
        name = clean_html(name_html)
        subscription_type = clean_html(match.group("subscription_type"))
        version = clean_html(match.group("version"))
        description = clean_html(match.group("description"))
        stability = classify_stability(name_html)
        subscriptions.append(
            {
                "id": snake_case(f"{subscription_type}_{version}"),
                "name": name,
                "subscription_type": subscription_type,
                "version": version,
                "description": description,
                "stability": stability,
            }
        )

    return {
        "generated_at": now_iso(),
        "source": SOURCE_URLS["eventsub_types"],
        "subscriptions": subscriptions,
    }


def build_summary(helix: dict[str, Any], eventsub: dict[str, Any], auth: dict[str, Any]) -> dict[str, Any]:
    helix_endpoints = helix["endpoints"]
    eventsub_subscriptions = eventsub["subscriptions"]
    return {
        "generated_at": now_iso(),
        "sources": SOURCE_URLS,
        "counts": {
            "helix_total": len(helix_endpoints),
            "helix_ga_or_new": sum(1 for item in helix_endpoints if item["stability"] != "beta"),
            "helix_beta": sum(1 for item in helix_endpoints if item["stability"] == "beta"),
            "helix_groups": len(helix["group_counts"]),
            "eventsub_total": len(eventsub_subscriptions),
            "eventsub_ga_or_new": sum(
                1 for item in eventsub_subscriptions if item["stability"] != "beta"
            ),
            "eventsub_beta": sum(
                1 for item in eventsub_subscriptions if item["stability"] == "beta"
            ),
            "auth_flows": len(auth["flows"]),
            "auth_endpoints": len(auth["endpoints"]),
        },
    }


def render_helix_rust(helix: dict[str, Any]) -> str:
    groups: dict[str, list[dict[str, Any]]] = collections.OrderedDict()
    for endpoint in helix["endpoints"]:
        groups.setdefault(endpoint["group"], []).append(endpoint)

    out: list[str] = []
    out.append("// @generated by tools/twitch_surface.py. DO NOT EDIT BY HAND.")
    out.append("#![allow(dead_code)]")
    out.append("use super::{EndpointStability, HelixAuthKind, HelixEndpoint};")
    out.append("use crate::http::HttpMethod;")
    out.append("")

    all_paths: list[str] = []
    for group, endpoints in groups.items():
        module_name = snake_case(group)
        out.append(f"pub mod {module_name} {{")
        out.append("    use super::*;")
        out.append("")
        for endpoint in endpoints:
            const_name = snake_case(endpoint["id"]).upper()
            request_name = camel_case(endpoint["name"]) + "Request"
            response_name = camel_case(endpoint["name"]) + "Response"
            description = endpoint["description"].replace("\n", " ")
            description = description.replace("*/", "* /")
            scopes = endpoint["auth"]["scopes"]
            scopes_expr = "&[" + ", ".join(rust_string(scope) for scope in scopes) + "]"
            out.append(f"    /// {description}")
            out.append(f"    pub const {const_name}: HelixEndpoint = HelixEndpoint {{")
            out.append(f"        id: {rust_string(endpoint['id'])},")
            out.append(f"        group: {rust_string(endpoint['group'])},")
            out.append(f"        name: {rust_string(endpoint['name'])},")
            out.append(f"        description: {rust_string(description)},")
            out.append(
                f"        stability: EndpointStability::{camel_case(endpoint['stability'])},"
            )
            out.append(f"        method: HttpMethod::{camel_case(endpoint['method'])},")
            out.append(f"        path: {rust_string(endpoint['path'])},")
            out.append(
                f"        auth_kind: HelixAuthKind::{camel_case(endpoint['auth']['kind'])},"
            )
            out.append(f"        scopes: {scopes_expr},")
            out.append(
                f"        supports_pagination: {'true' if endpoint['supports_pagination'] else 'false'},"
            )
            out.append("    };")
            out.append(
                f"    declare_generated_endpoint!({request_name}, {response_name}, {const_name});"
            )
            out.append("")
            all_paths.append(f"&{module_name}::{const_name}")
        out.append("}")
        out.append("")

    out.append("pub static ALL_ENDPOINTS: &[&HelixEndpoint] = &[")
    for path in all_paths:
        out.append(f"    {path},")
    out.append("];")
    out.append("")
    return "\n".join(out)


def render_eventsub_rust(eventsub: dict[str, Any]) -> str:
    subscriptions = eventsub["subscriptions"]
    variants = []
    aliases = []
    match_arms = []
    all_entries = []

    for item in subscriptions:
        variant = camel_case(item["subscription_type"].replace(".", "_")) + camel_case(
            item["version"]
        )
        alias = variant + "Event"
        stability = camel_case(item["stability"])
        subscription_type = item["subscription_type"]
        version = item["version"]
        description = item["description"].replace("\n", " ")
        variants.append((variant, description))
        aliases.append((alias, stability))
        if subscription_type == "channel.chat.message" and version == "1":
            match_arms.append(
                f'        ({rust_string(subscription_type)}, Some({rust_string(version)})) => decode_typed_event::<EventSubChatMessage>(event).map(KnownEventSubPayload::{variant}),'
            )
        elif subscription_type == "channel.chat.message_delete" and version == "1":
            match_arms.append(
                f'        ({rust_string(subscription_type)}, Some({rust_string(version)})) => decode_typed_event::<EventSubChatMessageDeleted>(event).map(KnownEventSubPayload::{variant}),'
            )
        else:
            match_arms.append(
                f'        ({rust_string(subscription_type)}, Some({rust_string(version)})) => decode_generic_event(event, source_timestamp).map(KnownEventSubPayload::{variant}),'
            )
        all_entries.append(
            "    EventSubSubscriptionDefinition { "
            + f"id: {rust_string(item['id'])}, "
            + f"name: {rust_string(item['name'])}, "
            + f"subscription_type: {rust_string(subscription_type)}, "
            + f"version: {rust_string(version)}, "
            + f"stability: EndpointStability::{stability}, "
            + f"description: {rust_string(description)}"
            + " },"
        )

    out: list[str] = []
    out.append("// @generated by tools/twitch_surface.py. DO NOT EDIT BY HAND.")
    out.append("#![allow(dead_code)]")
    out.append("use std::collections::BTreeMap;")
    out.append("")
    out.append("use serde::{Deserialize, Serialize};")
    out.append("use time::OffsetDateTime;")
    out.append("")
    out.append("use super::{")
    out.append("    EndpointStability, EventSubChatMessage, EventSubChatMessageDeleted,")
    out.append("    EventSubSubscriptionDefinition,")
    out.append("};")
    out.append("")
    out.append("#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]")
    out.append("pub struct GenericEventSubPayload {")
    out.append("    #[serde(flatten)]")
    out.append("    pub fields: BTreeMap<String, serde_json::Value>,")
    out.append("    #[serde(skip)]")
    out.append("    pub source_timestamp: Option<OffsetDateTime>,")
    out.append("}")
    out.append("")
    for alias, _ in aliases:
        out.append(f"pub type {alias} = GenericEventSubPayload;")
    out.append("")
    out.append("#[allow(clippy::large_enum_variant)]")
    out.append("#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]")
    out.append("pub enum KnownEventSubPayload {")
    for variant, description in variants:
        if variant == "ChannelChatMessage1":
            out.append(f"    /// {description}")
            out.append(f"    {variant}(EventSubChatMessage),")
        elif variant == "ChannelChatMessageDelete1":
            out.append(f"    /// {description}")
            out.append(f"    {variant}(EventSubChatMessageDeleted),")
        else:
            out.append(f"    /// {description}")
            out.append(f"    {variant}(GenericEventSubPayload),")
    out.append("}")
    out.append("")
    out.append("pub static ALL_SUBSCRIPTIONS: &[EventSubSubscriptionDefinition] = &[")
    out.extend(all_entries)
    out.append("];")
    out.append("")
    out.append("pub(crate) fn decode_known_payload(")
    out.append("    subscription_type: &str,")
    out.append("    version: Option<&str>,")
    out.append("    event: Option<serde_json::Value>,")
    out.append("    source_timestamp: Option<OffsetDateTime>,")
    out.append(") -> Option<KnownEventSubPayload> {")
    out.append("    match (subscription_type, version) {")
    out.extend(match_arms)
    out.append("        _ => None,")
    out.append("    }")
    out.append("}")
    out.append("")
    out.append(
        "fn decode_typed_event<T: serde::de::DeserializeOwned>(event: Option<serde_json::Value>) -> Option<T> {"
    )
    out.append("    serde_json::from_value(event?).ok()")
    out.append("}")
    out.append("")
    out.append("fn decode_generic_event(")
    out.append("    event: Option<serde_json::Value>,")
    out.append("    source_timestamp: Option<OffsetDateTime>,")
    out.append(") -> Option<GenericEventSubPayload> {")
    out.append("    let mut payload: GenericEventSubPayload = serde_json::from_value(event?).ok()?;")
    out.append("    payload.source_timestamp = source_timestamp;")
    out.append("    Some(payload)")
    out.append("}")
    out.append("")
    return "\n".join(out)


def write_json(path: pathlib.Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def build_command(args: argparse.Namespace) -> int:
    reference_html = pathlib.Path(args.reference_html).read_text()
    eventsub_html = pathlib.Path(args.eventsub_html).read_text()
    helix = parse_helix(reference_html)
    eventsub = parse_eventsub(eventsub_html)
    auth = {
        "generated_at": now_iso(),
        "source_urls": SOURCE_URLS,
        **AUTH_SURFACE,
    }
    summary = build_summary(helix, eventsub, auth)

    catalog_dir = pathlib.Path(args.catalog_dir)
    write_json(catalog_dir / "helix.json", helix)
    write_json(catalog_dir / "eventsub.json", eventsub)
    write_json(catalog_dir / "auth.json", auth)
    write_json(catalog_dir / "summary.json", summary)

    helix_rs = pathlib.Path(args.helix_rust)
    helix_rs.parent.mkdir(parents=True, exist_ok=True)
    helix_rs.write_text(render_helix_rust(helix))

    eventsub_rs = pathlib.Path(args.eventsub_rust)
    eventsub_rs.parent.mkdir(parents=True, exist_ok=True)
    eventsub_rs.write_text(render_eventsub_rust(eventsub))

    print(
        json.dumps(
            {
                "helix_total": summary["counts"]["helix_total"],
                "eventsub_total": summary["counts"]["eventsub_total"],
                "catalog_dir": str(catalog_dir),
            }
        )
    )
    return 0


def load_catalog_dir(path: pathlib.Path) -> dict[str, Any]:
    return {
        "helix": json.loads((path / "helix.json").read_text()),
        "eventsub": json.loads((path / "eventsub.json").read_text()),
        "auth": json.loads((path / "auth.json").read_text()),
        "summary": json.loads((path / "summary.json").read_text()),
    }


def diff_catalogs(old: dict[str, Any], new: dict[str, Any]) -> tuple[str, int]:
    lines = ["# Twitch Surface Drift Report", ""]
    severity = 0

    def diff_named(kind: str, old_items: list[dict[str, Any]], new_items: list[dict[str, Any]]) -> None:
        nonlocal severity
        old_by_id = {item["id"]: item for item in old_items}
        new_by_id = {item["id"]: item for item in new_items}
        added = sorted(set(new_by_id) - set(old_by_id))
        removed = sorted(set(old_by_id) - set(new_by_id))
        changed = []
        for item_id in sorted(set(old_by_id) & set(new_by_id)):
            if old_by_id[item_id] != new_by_id[item_id]:
                changed.append(item_id)

        lines.append(f"## {kind}")
        lines.append("")
        if not added and not removed and not changed:
            lines.append("No drift detected.")
            lines.append("")
            return

        if added:
            severity = max(severity, 2)
            lines.append("### Added")
            for item_id in added:
                lines.append(f"- `{item_id}`")
            lines.append("")
        if removed:
            severity = max(severity, 3)
            lines.append("### Removed")
            for item_id in removed:
                lines.append(f"- `{item_id}`")
            lines.append("")
        if changed:
            severity = max(severity, 2)
            lines.append("### Changed")
            for item_id in changed:
                before = old_by_id[item_id]
                after = new_by_id[item_id]
                differing = sorted(
                    key
                    for key in sorted(set(before) | set(after))
                    if before.get(key) != after.get(key)
                )
                lines.append(f"- `{item_id}` changed fields: {', '.join(differing)}")
            lines.append("")

    diff_named("Helix", old["helix"]["endpoints"], new["helix"]["endpoints"])
    diff_named(
        "EventSub",
        old["eventsub"]["subscriptions"],
        new["eventsub"]["subscriptions"],
    )

    old_counts = old["summary"]["counts"]
    new_counts = new["summary"]["counts"]
    if old_counts != new_counts:
        severity = max(severity, 2)
        lines.append("## Count Changes")
        lines.append("")
        for key in sorted(set(old_counts) | set(new_counts)):
            if old_counts.get(key) != new_counts.get(key):
                lines.append(f"- `{key}`: `{old_counts.get(key)}` -> `{new_counts.get(key)}`")
        lines.append("")

    lines.append(f"Severity: `{severity}`")
    lines.append("")
    lines.append("Severity guide: `0` none, `1` additive metadata, `2` surface changed, `3` removed/breaking.")
    lines.append("")
    return "\n".join(lines), severity


def diff_command(args: argparse.Namespace) -> int:
    old_catalog = load_catalog_dir(pathlib.Path(args.old_catalog))
    new_catalog = load_catalog_dir(pathlib.Path(args.new_catalog))
    report, severity = diff_catalogs(old_catalog, new_catalog)
    if args.output:
        pathlib.Path(args.output).write_text(report)
    else:
        sys.stdout.write(report)
    return 0 if severity < args.fail_at_or_above else severity


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Generate Twitch surface catalogs and Rust registries.")
    subparsers = parser.add_subparsers(dest="command", required=True)

    build = subparsers.add_parser("build", help="Build catalog JSON and generated Rust registries.")
    build.add_argument("--reference-html", required=True)
    build.add_argument("--eventsub-html", required=True)
    build.add_argument("--catalog-dir", required=True)
    build.add_argument("--helix-rust", required=True)
    build.add_argument("--eventsub-rust", required=True)
    build.set_defaults(func=build_command)

    diff = subparsers.add_parser("diff", help="Diff two generated catalog directories.")
    diff.add_argument("--old-catalog", required=True)
    diff.add_argument("--new-catalog", required=True)
    diff.add_argument("--output")
    diff.add_argument("--fail-at-or-above", type=int, default=3)
    diff.set_defaults(func=diff_command)

    return parser


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
