#!/usr/bin/env python3
from __future__ import annotations

import argparse
import collections
import copy
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
    "eventsub_reference": "https://dev.twitch.tv/docs/eventsub/eventsub-reference/",
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


def strip_tags(raw: str) -> str:
    return html.unescape(re.sub(r"<[^>]+>", "", raw))


def clean_html(raw: str) -> str:
    text = re.sub(r"<br\s*/?>", "\n", raw, flags=re.I)
    text = re.sub(r"</(p|div|li|ul|ol|tr|td|th|h2|h3|h4)>", "\n", text, flags=re.I)
    lines = (re.sub(r"\s+", " ", line).strip() for line in strip_tags(text).splitlines())
    return "\n".join(line for line in lines if line)


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


def parse_section_raw(section_html: str, heading: str) -> str:
    pattern = re.compile(
        rf"<h3[^>]*>\s*{re.escape(heading)}\s*</h3>(?P<body>.*?)(?=<h[23][^>]*>|$)",
        re.I | re.S,
    )
    match = pattern.search(section_html)
    return match.group("body") if match else ""


def parse_section(section_html: str, heading: str) -> str:
    body = parse_section_raw(section_html, heading)
    return clean_html(body) if body else ""


RESPONSE_TABLE_RE = re.compile(r"<table[^>]*>.*?</table>", re.S)
# Fragment of the first link in a cell; the docs link names to section ids.
TYPE_LINK_RE = re.compile(r'href="[^"]*#(?P<slug>[^"]+)"')
EVENT_LINK_RE = re.compile(r'href="[^"]*eventsub-reference/?#(?P<slug>[^"]+)"')
RESPONSE_ROW_RE = re.compile(
    r"<tr>\s*<td[^>]*>(?P<field>.*?)</td>\s*<td[^>]*>(?P<type>.*?)</td>\s*<td[^>]*>(?P<description>.*?)</td>\s*</tr>",
    re.S,
)
INDENT_CHARS = {" ", "\xa0", "\t"}
# Documented types that cannot have nested child rows; used to repair rows the
# docs over-indent under a scalar sibling.
SCALAR_RESPONSE_TYPES = {
    "string",
    "integer",
    "int64",
    "unsigned integer",
    "float",
    "boolean",
    "string[]",
    "integer[]",
}


def response_type_can_nest(type_str: str) -> bool:
    return type_str.strip().lower() not in SCALAR_RESPONSE_TYPES


# EventSub reference docs use a looser type vocabulary than Helix; lookup is on
# the lowercased raw cell so the catalog stores canonical type names.
EVENTSUB_TYPE_ALIASES = {
    "str": "string",
    "int": "integer",
    "int (or null)": "integer",
    "bool": "boolean",
    "[]string": "string[]",
}
EVENTSUB_PRIMITIVE_TYPES = {"string", "integer", "boolean", "string[]", "integer[]", "array", "float"}

# Events that keep their hand-written structs in src/eventsub.rs, keyed to the
# struct name; their fields are still scraped into the catalog for completeness.
HANDWRITTEN_EVENTS = {
    ("channel.chat.message", "1"): "EventSubChatMessage",
    ("channel.chat.message_delete", "1"): "EventSubChatMessageDeleted",
}

# Shared doc object sections emitted once as `Shared*` structs, keyed by the
# reference-page section id: (struct name, documented as an array). Whether a
# section is an array is not derivable from its Type cell, so it is asserted
# here (a description-based tripwire warns on drift); array sections name the
# element struct in the singular.
SHARED_SECTIONS = {
    "choices": ("SharedChoice", True),
    "outcomes": ("SharedOutcome", True),
    "top-predictors": ("SharedTopPredictor", True),
    "emotes": ("SharedEmote", True),
    "reward": ("SharedReward", False),
    "image": ("SharedImage", False),
    "message": ("SharedMessage", False),
    "max-per-stream": ("SharedMaxPerStream", False),
    "max-per-user-per-stream": ("SharedMaxPerUserPerStream", False),
    "global-cooldown": ("SharedGlobalCooldown", False),
    "bits-voting": ("SharedBitsVoting", False),
    "channel-points-voting": ("SharedChannelPointsVoting", False),
    "custom-power-up": ("SharedCustomPowerUp", False),
    "product": ("SharedProduct", False),
    "last-contribution": ("SharedLastContribution", False),
    "top-contributions": ("SharedTopContribution", True),
}


def shared_section_slug(type_str: str) -> str:
    """Kebab-cases a named type's display text (`custom_power_up`, `Custom Power
    Up`) into the reference-page section id; used when the cell has no link."""
    return re.sub(r"[^a-z0-9]+", "-", type_str.lower()).strip("-")


def shared_section_is_array(slug: str) -> bool:
    return SHARED_SECTIONS.get(slug, ("", False))[1]


def normalize_eventsub_type(raw: str) -> str:
    stripped = raw.strip()
    lowered = stripped.lower()
    if lowered in EVENTSUB_TYPE_ALIASES:
        return EVENTSUB_TYPE_ALIASES[lowered]
    if lowered in EVENTSUB_PRIMITIVE_TYPES or lowered in {"object", "object[]"}:
        return lowered
    return stripped


def response_field_indent_and_name(raw_field: str) -> tuple[int, str]:
    text = strip_tags(raw_field)
    unindented = text.lstrip("".join(INDENT_CHARS))
    return len(text) - len(unindented), unindented.strip()


def parse_fields_table(
    table_html: str,
    context_id: str,
    warnings: list[str],
    *,
    scalar_parent: str = "lift",
    normalize: Any = None,
    capture_refs: bool = False,
) -> list[dict[str, Any]] | None:
    rows = RESPONSE_ROW_RE.findall(table_html)
    if not rows:
        warnings.append(f"{context_id}: response table has no parseable rows")
        return None

    fields: list[dict[str, Any]] = []
    # Stack of (indent, field) along the current ancestor path. Indent widths
    # in the docs are irregular (2/3/5/7/9 characters), so depth is inferred
    # relatively: an indent wider than the stack top nests one level deeper,
    # except that a scalar-typed field cannot be a parent. Helix tables repair
    # that case by lifting the row to a sibling ("lift"); EventSub tables
    # instead document real nesting under a mistyped scalar, so the parent is
    # retyped to an object ("retype").
    stack: list[tuple[int, dict[str, Any]]] = []
    for raw_field, raw_type, raw_description in rows:
        indent, name = response_field_indent_and_name(raw_field)
        if not name:
            warnings.append(f"{context_id}: empty field name in response table")
            return None
        type_str = strip_tags(raw_type).strip()
        if normalize is not None:
            type_str = normalize(type_str)
        field: dict[str, Any] = {
            "name": name,
            "type": type_str,
            "description": clean_html(raw_description).replace("\n", " ").strip(),
        }
        # EventSub type cells link named types to their reference section; the
        # fragment is the section id, which `resolve()` prefers over the text.
        link = TYPE_LINK_RE.search(raw_type) if capture_refs else None
        if link:
            field["ref"] = link.group("slug")
        while stack and stack[-1][0] >= indent:
            stack.pop()
        if scalar_parent == "lift":
            while stack and not response_type_can_nest(stack[-1][1]["type"]):
                stack.pop()
        elif stack:
            parent = stack[-1][1]
            parent_type = parent["type"]
            if not response_type_can_nest(parent_type) or parent_type == "array":
                retyped = (
                    "object[]"
                    if parent_type.endswith("[]") or parent_type == "array"
                    else "object"
                )
                warnings.append(
                    f"{context_id}: retyped scalar parent `{parent['name']}` "
                    f"({parent_type}) to {retyped}"
                )
                parent["type"] = retyped
        if stack:
            stack[-1][1].setdefault("children", []).append(field)
        else:
            fields.append(field)
        stack.append((indent, field))
    return fields


def parse_response_body(
    section_html: str, endpoint_id: str, warnings: list[str]
) -> list[dict[str, Any]] | None:
    body = parse_section_raw(section_html, "Response Body")
    if not body:
        return None
    table_match = RESPONSE_TABLE_RE.search(body)
    if not table_match:
        return None
    return parse_fields_table(table_match.group(0), endpoint_id, warnings)


def parse_expected_status(section_html: str) -> int:
    body = parse_section(section_html, "Response Codes")
    if not body:
        return 200
    match = re.search(r"\b([1-5]\d{2})\b", body)
    return int(match.group(1)) if match else 200


def parse_helix(
    reference_html: str, warnings: list[str], *, generated_at: str
) -> dict[str, Any]:
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
        lowered_body = body.lower()
        supports_pagination = any(
            needle in lowered_body for needle in ("pagination", " after ", " before ", "cursor")
        )
        endpoint_id = snake_case(f"{group}_{name}")
        expected_status = parse_expected_status(body)
        response_fields = parse_response_body(body, endpoint_id, warnings)
        if response_fields is not None:
            response = {"fields": response_fields}
        elif expected_status == 204:
            response = {"fields": []}
        else:
            response = None
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
                "expected_status": expected_status,
                "response": response,
            }
        )
        groups[group] += 1

    return {
        "generated_at": generated_at,
        "source": SOURCE_URLS["helix_reference"],
        "group_counts": dict(sorted(groups.items())),
        "endpoints": endpoints,
    }


def event_section_slugs(eventsub_html: str, anchors: list[str]) -> dict[str, str | None]:
    """Maps each subscription's anchor on the types page to the reference-page
    section its payload table links the `event` row to."""
    starts = {anchor: eventsub_html.find(f'<h3 id="{anchor}"') for anchor in anchors}
    ordered = sorted(start for start in starts.values() if start != -1)
    slugs: dict[str, str | None] = {}
    for anchor, start in starts.items():
        if start == -1:
            slugs[anchor] = None
            continue
        end = next((position for position in ordered if position > start), len(eventsub_html))
        # The payload row is `event`, or `events` for batched subscriptions;
        # either way its Type cell links to the reference section.
        rows = RESPONSE_ROW_RE.findall(eventsub_html[start:end])
        slugs[anchor] = next(
            (
                link.group("slug")
                for raw_field, raw_type, _ in rows
                if strip_tags(raw_field).strip() in {"event", "events"}
                and (link := EVENT_LINK_RE.search(raw_type))
            ),
            None,
        )
    return slugs


def parse_eventsub(eventsub_html: str, *, generated_at: str) -> dict[str, Any]:
    start = eventsub_html.find('<h1 id="subscription-types">')
    if start == -1:
        raise RuntimeError("failed to locate EventSub subscription table")
    end = eventsub_html.find("## Public Beta Program")
    summary_html = eventsub_html[start:end if end != -1 else None]

    row_re = re.compile(
        r"<tr>\s*<td>(?P<name>.*?)</td>\s*<td><code[^>]*>(?P<subscription_type>[^<]+)</code>\s*</td>\s*<td><code[^>]*>(?P<version>[^<]+)</code>\s*</td>\s*<td>(?P<description>.*?)</td>\s*</tr>",
        re.S,
    )

    rows = list(row_re.finditer(summary_html))
    anchors = [
        link.group("slug") if (link := TYPE_LINK_RE.search(match.group("name"))) else None
        for match in rows
    ]
    event_sections = event_section_slugs(eventsub_html, [a for a in anchors if a])

    subscriptions = []
    for match, anchor in zip(rows, anchors):
        name_html = match.group("name")
        subscription_type = clean_html(match.group("subscription_type"))
        version = clean_html(match.group("version"))
        subscriptions.append(
            {
                "id": snake_case(f"{subscription_type}_{version}"),
                "name": clean_html(name_html),
                "subscription_type": subscription_type,
                "version": version,
                "description": clean_html(match.group("description")),
                "stability": classify_stability(name_html),
                "event_section": event_sections.get(anchor) if anchor else None,
            }
        )

    return {
        "generated_at": generated_at,
        "source": SOURCE_URLS["eventsub_types"],
        "subscriptions": subscriptions,
    }


def attach_eventsub_events(
    eventsub: dict[str, Any], reference_html: str, warnings: list[str]
) -> dict[str, Any]:
    """Attaches `event` field shapes from the EventSub reference page to each
    subscription and returns the shared object sections referenced by name."""
    heading_re = re.compile(r"<h[23] id=\"(?P<slug>[^\"]+)\"[^>]*>")
    matches = list(heading_re.finditer(reference_html))
    sections: dict[str, str] = {}
    for index, match in enumerate(matches):
        end = matches[index + 1].start() if index + 1 < len(matches) else len(reference_html)
        sections[match.group("slug")] = reference_html[match.start() : end]

    parsed: dict[str, list[dict[str, Any]] | None] = {}
    resolving: set[str] = set()
    shared_used: dict[str, list[dict[str, Any]]] = {}

    def section_fields(slug: str) -> list[dict[str, Any]] | None:
        if slug in parsed:
            return parsed[slug]
        body = sections.get(slug)
        table_match = RESPONSE_TABLE_RE.search(body) if body else None
        fields = (
            parse_fields_table(
                table_match.group(0),
                slug,
                warnings,
                scalar_parent="retype",
                normalize=normalize_eventsub_type,
                capture_refs=True,
            )
            if table_match
            else None
        )
        parsed[slug] = fields
        return fields

    def resolve(fields: list[dict[str, Any]], origin: str) -> None:
        for field in fields:
            type_str = field["type"]
            lowered = type_str.lower()
            is_named = (
                lowered not in EVENTSUB_PRIMITIVE_TYPES
                and lowered not in {"object", "object[]"}
            )
            # Only a named type that resolves to a shared section keeps `ref`.
            slug = field.pop("ref", None) or shared_section_slug(type_str)
            if field.get("children"):
                if is_named:
                    field["type"] = "object[]" if shared_section_is_array(slug) else "object"
                resolve(field["children"], origin)
                continue
            if not is_named:
                continue
            if slug in resolving:
                warnings.append(f"{origin}: cyclic named type `{type_str}`")
                continue
            resolving.add(slug)
            shared_fields = section_fields(slug)
            if shared_fields is not None and slug not in shared_used:
                resolve(shared_fields, slug)
                shared_used[slug] = shared_fields
            resolving.discard(slug)
            if shared_fields is None:
                warnings.append(f"{origin}: unresolved named type `{type_str}` (section `{slug}`)")
                continue
            field["ref"] = slug
            is_array = shared_section_is_array(slug)
            field["type"] = "object[]" if is_array else "object"
            if not is_array and "array of" in field["description"].lower():
                warnings.append(
                    f"{origin}: field `{field['name']}` described as an array but "
                    f"section `{slug}` is not marked as an array in SHARED_SECTIONS"
                )

    consumed: set[str] = set()
    for item in eventsub["subscriptions"]:
        slug = item.get("event_section")
        fields = section_fields(slug) if slug else None
        if fields is None:
            where = f"`{slug}` has no table" if slug else "types page has no `event` link"
            warnings.append(f"{item['id']}: no event section found ({where})")
            item["event"] = None
            continue
        consumed.add(slug)
        resolve(fields, slug)
        item["event"] = {"fields": copy.deepcopy(fields)}

    for slug in sections:
        if (slug.endswith("-event") or slug.endswith("-event-v2")) and slug not in consumed:
            warnings.append(f"event section `{slug}` is not consumed by any subscription")

    eventsub["event_source"] = SOURCE_URLS["eventsub_reference"]
    eventsub["shared_objects"] = {
        slug: {"fields": fields} for slug, fields in sorted(shared_used.items())
    }
    return shared_used


def build_summary(
    helix: dict[str, Any], eventsub: dict[str, Any], auth: dict[str, Any], *, generated_at: str
) -> dict[str, Any]:
    helix_endpoints = helix["endpoints"]
    eventsub_subscriptions = eventsub["subscriptions"]
    return {
        "generated_at": generated_at,
        "sources": SOURCE_URLS,
        "counts": {
            "helix_total": len(helix_endpoints),
            "helix_ga_or_new": sum(1 for item in helix_endpoints if item["stability"] != "beta"),
            "helix_beta": sum(1 for item in helix_endpoints if item["stability"] == "beta"),
            "helix_groups": len(helix["group_counts"]),
            "helix_typed_responses": sum(
                1 for item in helix_endpoints if item["response"] is not None
            ),
            "helix_untyped_responses": sum(
                1 for item in helix_endpoints if item["response"] is None
            ),
            "eventsub_total": len(eventsub_subscriptions),
            "eventsub_ga_or_new": sum(
                1 for item in eventsub_subscriptions if item["stability"] != "beta"
            ),
            "eventsub_beta": sum(
                1 for item in eventsub_subscriptions if item["stability"] == "beta"
            ),
            "eventsub_typed_events": sum(
                1 for item in eventsub_subscriptions if item["event"] is not None
            ),
            "eventsub_untyped_events": sum(
                1 for item in eventsub_subscriptions if item["event"] is None
            ),
            "auth_flows": len(auth["flows"]),
            "auth_endpoints": len(auth["endpoints"]),
        },
    }


# Keywords that cannot be raw identifiers; anything else keyword-like becomes
# `r#name`, which serde serializes under the bare name without a rename.
NON_RAW_IDENT_KEYWORDS = {"crate", "self", "super"}
RUST_KEYWORDS = {
    "abstract", "as", "async", "await", "become", "box", "break", "const",
    "continue", "crate", "do", "dyn", "else", "enum", "extern", "false",
    "final", "fn", "for", "gen", "if", "impl", "in", "let", "loop", "macro",
    "match", "mod", "move", "mut", "override", "priv", "pub", "ref", "return",
    "self", "static", "struct", "super", "trait", "true", "try", "type",
    "typeof", "unsafe", "unsized", "use", "virtual", "where", "while", "yield",
}
MAP_RESPONSE_TYPES = {
    "map[string]string": "std::collections::BTreeMap<String, String>",
    "map[string,string]": "std::collections::BTreeMap<String, String>",
    "map[string]object": "std::collections::BTreeMap<String, serde_json::Value>",
    "dictionary": "std::collections::BTreeMap<String, serde_json::Value>",
}
PRIMITIVE_RESPONSE_TYPES = {
    "string": "String",
    "integer": "i64",
    "int64": "i64",
    "unsigned integer": "i64",
    "float": "f64",
    "boolean": "bool",
    "string[]": "Vec<String>",
    "integer[]": "Vec<i64>",
    "array": "Vec<serde_json::Value>",
}


class ResponseRenderError(Exception):
    pass


def rust_field_ident(name: str) -> tuple[str, str | None]:
    """Returns (identifier, serde_rename or None)."""
    ident = snake_case(name)
    if not ident:
        raise ResponseRenderError(f"field name `{name}` has no valid identifier")
    if ident[0].isdigit():
        ident = f"n{ident}"
    rename = name if ident != name else None
    if ident in RUST_KEYWORDS:
        if ident in NON_RAW_IDENT_KEYWORDS:
            rename = name
            ident = f"{ident}_field"
        else:
            ident = f"r#{ident}"
    return ident, rename


def rust_type_for(field: dict[str, Any], child_struct: str | None) -> str:
    type_str = field["type"].strip()
    lowered = type_str.lower()
    if lowered in PRIMITIVE_RESPONSE_TYPES:
        return PRIMITIVE_RESPONSE_TYPES[lowered]
    if lowered in MAP_RESPONSE_TYPES:
        return MAP_RESPONSE_TYPES[lowered]
    is_array = type_str.endswith("[]")
    if child_struct:
        return f"Vec<{child_struct}>" if is_array else child_struct
    return "Vec<serde_json::Value>" if is_array else "serde_json::Value"


def doc_sentence(description: str) -> str:
    sentence = description.split(". ")[0].strip()
    if sentence and not sentence.endswith("."):
        sentence += "."
    if len(sentence) > 200:
        sentence = sentence[:197] + "..."
    return sentence


def render_struct_tree(
    rendered: list[str],
    struct_names: set[str],
    struct_name: str,
    child_prefix: str,
    fields: list[dict[str, Any]],
    doc: str,
    *,
    indent: str,
    derives: str,
    field_attr: Any,
    special_field: Any = None,
    child_namer: Any = None,
    ref_struct: Any = None,
    root_extra_lines: tuple[str, ...] = (),
    is_root: bool = True,
) -> None:
    """Renders one struct and, depth-first, the nested structs its fields need.

    Raises ResponseRenderError when the documented shape cannot be expressed.
    """
    if struct_name in struct_names:
        raise ResponseRenderError(f"struct name `{struct_name}` collides")
    struct_names.add(struct_name)

    lines: list[str] = []
    pending: list[tuple[str, str, list[dict[str, Any]], str]] = []
    seen_idents: set[str] = set()
    lines.append(f"{indent}/// {doc}")
    lines.append(f"{indent}#[derive({derives})]")
    lines.append(f"{indent}pub struct {struct_name} {{")
    for field in fields:
        name = field["name"]
        ident, rename = rust_field_ident(name)
        if ident in seen_idents:
            raise ResponseRenderError(f"duplicate field `{ident}` in `{struct_name}`")
        seen_idents.add(ident)
        lowered = field["type"].strip().lower()
        rust_type = special_field(field, is_root) if special_field is not None else None
        if rust_type is None and ref_struct is not None:
            rust_type = ref_struct(field)
        if rust_type is None:
            child_struct = None
            if field.get("children") and lowered not in MAP_RESPONSE_TYPES:
                if child_namer is not None:
                    child_struct, grandchild_prefix = child_namer(
                        field, is_root, child_prefix
                    )
                else:
                    child_struct = f"{child_prefix}{camel_case(name)}"
                    grandchild_prefix = child_struct
                field_doc = doc_sentence(field["description"]) or f"`{name}` object."
                pending.append(
                    (child_struct, grandchild_prefix, field["children"], field_doc)
                )
            rust_type = rust_type_for(field, child_struct)
        description = doc_sentence(field["description"])
        if description:
            lines.append(f"{indent}    /// {description}")
        for attr in field_attr(rename):
            lines.append(f"{indent}    {attr}")
        lines.append(f"{indent}    pub {ident}: {rust_type},")
    if is_root:
        for line in root_extra_lines:
            lines.append(f"{indent}    {line}")
    lines.append(f"{indent}}}")
    lines.append("")
    rendered.extend(lines)

    for child_name, grandchild_prefix, child_fields, child_doc in pending:
        render_struct_tree(
            rendered,
            struct_names,
            child_name,
            grandchild_prefix,
            child_fields,
            child_doc,
            indent=indent,
            derives=derives,
            field_attr=field_attr,
            special_field=special_field,
            child_namer=child_namer,
            ref_struct=ref_struct,
            is_root=False,
        )


def response_struct_name(endpoint: dict[str, Any]) -> str:
    return camel_case(endpoint["name"]) + "Response"


def render_parse_impl(response_name: str, expected_status: int, body: list[str]) -> list[str]:
    return [
        f"    impl {response_name} {{",
        f"        pub const EXPECTED_STATUS: u16 = {expected_status};",
        "",
        "        pub fn parse(",
        "            response: crate::helix::RawResponse,",
        "        ) -> Result<Self, crate::helix::HelixError> {",
        *[f"            {line}" for line in body],
        "        }",
        "    }",
        "",
    ]


def helix_field_attr(rename: str | None) -> list[str]:
    if rename is not None:
        return [f"#[serde(default, rename = {rust_string(rename)})]"]
    return ["#[serde(default)]"]


def render_response_types(endpoint: dict[str, Any]) -> list[str]:
    """Renders the typed response structs for one endpoint.

    Raises ResponseRenderError when the documented shape cannot be expressed;
    the caller falls back to the untyped alias.
    """
    base = camel_case(endpoint["name"])
    response_name = response_struct_name(endpoint)
    rendered: list[str] = []

    def special_field(field: dict[str, Any], is_root: bool) -> str | None:
        if is_root and field["name"] == "pagination":
            return "crate::helix::HelixPagination"
        return None

    def child_namer(
        field: dict[str, Any], is_root: bool, child_prefix: str
    ) -> tuple[str, str]:
        if is_root and field["name"] == "data":
            return f"{base}Item", base
        child_struct = f"{child_prefix}{camel_case(field['name'])}"
        return child_struct, child_struct

    render_struct_tree(
        rendered,
        set(),
        response_name,
        base,
        endpoint["response"]["fields"],
        f"Response body for the \"{endpoint['name']}\" endpoint.",
        indent="    ",
        derives="Clone, Debug, Default, PartialEq, serde::Deserialize",
        field_attr=helix_field_attr,
        special_field=special_field,
        child_namer=child_namer,
    )

    rendered.extend(
        render_parse_impl(
            response_name,
            endpoint["expected_status"],
            ["crate::helix::parse_typed_response(response, Self::EXPECTED_STATUS)"],
        )
    )
    return rendered


def render_unit_response(endpoint: dict[str, Any]) -> list[str]:
    response_name = response_struct_name(endpoint)
    return [
        f"    /// Response for the \"{endpoint['name']}\" endpoint "
        f"(expects HTTP {endpoint['expected_status']} with an empty body).",
        "    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]",
        f"    pub struct {response_name};",
        "",
        *render_parse_impl(
            response_name,
            endpoint["expected_status"],
            [
                "crate::helix::parse_empty_response(response, Self::EXPECTED_STATUS)?;",
                "Ok(Self)",
            ],
        ),
    ]


def validate_responses(helix: dict[str, Any], warnings: list[str]) -> None:
    """Nulls out response shapes that cannot be rendered, keeping the catalog,
    summary counts, and generated Rust consistent."""
    for endpoint in helix["endpoints"]:
        response = endpoint.get("response")
        if response is None or not response["fields"]:
            continue
        try:
            render_response_types(endpoint)
        except ResponseRenderError as error:
            warnings.append(f"{endpoint['id']}: {error}; falling back to untyped")
            endpoint["response"] = None


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
                f"    declare_generated_endpoint!({request_name}, {const_name});"
            )
            out.append("")
            response = endpoint.get("response")
            if response is None:
                out.append(
                    f"    pub type {response_struct_name(endpoint)} = crate::helix::HelixJsonResponse;"
                )
                out.append("")
            elif not response["fields"]:
                out.extend(render_unit_response(endpoint))
            else:
                out.extend(render_response_types(endpoint))
            all_paths.append(f"&{module_name}::{const_name}")
        out.append("}")
        out.append("")

    out.append("pub static ALL_ENDPOINTS: &[&HelixEndpoint] = &[")
    for path in all_paths:
        out.append(f"    {path},")
    out.append("];")
    out.append("")
    return "\n".join(out)


# Names already defined in src/eventsub.rs or imported by the generated
# module; generated structs must not collide with them.
EVENTSUB_STRUCT_SEED = {
    "GenericEventSubPayload",
    "KnownEventSubPayload",
    "EventSubSubscriptionDefinition",
    *HANDWRITTEN_EVENTS.values(),
}

EVENT_SOURCE_TIMESTAMP_LINES = (
    "/// Timestamp from the delivery envelope; not part of the payload itself.",
    "#[serde(skip)]",
    "pub source_timestamp: Option<OffsetDateTime>,",
)


def eventsub_field_attr(rename: str | None) -> list[str]:
    if rename is not None:
        return [
            "#[serde(default, deserialize_with = \"super::null_default\", "
            f"rename = {rust_string(rename)})]"
        ]
    return ["#[serde(default, deserialize_with = \"super::null_default\")]"]


def eventsub_ref_struct(field: dict[str, Any]) -> str | None:
    ref = field.get("ref")
    if ref is None:
        return None
    section = SHARED_SECTIONS.get(ref)
    if section is None:
        raise ResponseRenderError(f"no shared struct name for section `{ref}`")
    struct_name, is_array = section
    return f"Vec<{struct_name}>" if is_array else struct_name


def eventsub_variant(item: dict[str, Any]) -> str:
    return camel_case(item["subscription_type"].replace(".", "_")) + camel_case(
        item["version"]
    )


def render_shared_event_structs(
    shared_objects: dict[str, Any], struct_names: set[str]
) -> list[str]:
    rendered: list[str] = []
    for slug in sorted(shared_objects):
        section = SHARED_SECTIONS.get(slug)
        if section is None:
            raise ResponseRenderError(f"no shared struct name for section `{slug}`")
        struct_name = section[0]
        render_struct_tree(
            rendered,
            struct_names,
            struct_name,
            struct_name,
            shared_objects[slug]["fields"],
            f"Shared `{slug}` object from the EventSub reference documentation.",
            indent="",
            derives="Clone, Debug, Default, PartialEq, Serialize, Deserialize",
            field_attr=eventsub_field_attr,
            ref_struct=eventsub_ref_struct,
        )
    return rendered


def render_event_struct(
    item: dict[str, Any], struct_names: set[str]
) -> list[str]:
    fields = item["event"]["fields"]
    if any(field["name"] == "source_timestamp" for field in fields):
        raise ResponseRenderError("documented field named `source_timestamp`")
    struct_name = f"{eventsub_variant(item)}Event"
    rendered: list[str] = []
    render_struct_tree(
        rendered,
        struct_names,
        struct_name,
        struct_name,
        fields,
        f"Event payload for `{item['subscription_type']}` version {item['version']}. "
        "Undocumented fields are dropped when decoding.",
        indent="",
        derives="Clone, Debug, Default, PartialEq, Serialize, Deserialize",
        field_attr=eventsub_field_attr,
        ref_struct=eventsub_ref_struct,
        root_extra_lines=EVENT_SOURCE_TIMESTAMP_LINES,
    )
    rendered.append(f"impl super::HasSourceTimestamp for {struct_name} {{")
    rendered.append(
        "    fn set_source_timestamp(&mut self, source_timestamp: Option<OffsetDateTime>) {"
    )
    rendered.append("        self.source_timestamp = source_timestamp;")
    rendered.append("    }")
    rendered.append("}")
    rendered.append("")
    return rendered


def validate_events(eventsub: dict[str, Any], warnings: list[str]) -> None:
    """Nulls out event shapes that cannot be rendered, keeping the catalog,
    summary counts, and generated Rust consistent."""
    shared_objects = eventsub.get("shared_objects", {})
    base_names = set(EVENTSUB_STRUCT_SEED)
    try:
        render_shared_event_structs(shared_objects, base_names)
    except ResponseRenderError as error:
        warnings.append(f"shared objects: {error}; falling back to untyped events")
        for item in eventsub["subscriptions"]:
            item["event"] = None
        eventsub["shared_objects"] = {}
        return
    for item in eventsub["subscriptions"]:
        key = (item["subscription_type"], item["version"])
        if item["event"] is None or key in HANDWRITTEN_EVENTS:
            continue
        try:
            render_event_struct(item, set(base_names))
        except ResponseRenderError as error:
            warnings.append(f"{item['id']}: {error}; falling back to untyped")
            item["event"] = None


def render_eventsub_rust(eventsub: dict[str, Any]) -> str:
    subscriptions = eventsub["subscriptions"]
    shared_objects = eventsub.get("shared_objects", {})
    variants = []
    aliases = []
    typed_blocks: list[str] = []
    match_arms = []
    all_entries = []

    base_names = set(EVENTSUB_STRUCT_SEED)
    shared_blocks = render_shared_event_structs(shared_objects, base_names)

    for item in subscriptions:
        variant = eventsub_variant(item)
        stability = camel_case(item["stability"])
        subscription_type = item["subscription_type"]
        version = item["version"]
        description = item["description"].replace("\n", " ")
        payload_type = HANDWRITTEN_EVENTS.get((subscription_type, version))
        if payload_type is None and item["event"] is not None:
            payload_type = f"{variant}Event"
            typed_blocks.extend(render_event_struct(item, set(base_names)))
        elif payload_type is None:
            payload_type = "GenericEventSubPayload"
            aliases.append(f"{variant}Event")

        variants.append((variant, description, payload_type))
        match_arms.append(
            f'        ({rust_string(subscription_type)}, Some({rust_string(version)})) => decode_typed_event::<{payload_type}>(event, source_timestamp).map(KnownEventSubPayload::{variant}),'
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
    handwritten = ", ".join(sorted(set(HANDWRITTEN_EVENTS.values())))
    out.append(
        f"use super::{{EndpointStability, EventSubSubscriptionDefinition, {handwritten}}};"
    )
    out.append("")
    out.append("#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]")
    out.append("pub struct GenericEventSubPayload {")
    out.append("    #[serde(flatten)]")
    out.append("    pub fields: BTreeMap<String, serde_json::Value>,")
    out.append("    #[serde(skip)]")
    out.append("    pub source_timestamp: Option<OffsetDateTime>,")
    out.append("}")
    out.append("")
    out.append("impl super::HasSourceTimestamp for GenericEventSubPayload {")
    out.append("    fn set_source_timestamp(&mut self, source_timestamp: Option<OffsetDateTime>) {")
    out.append("        self.source_timestamp = source_timestamp;")
    out.append("    }")
    out.append("}")
    out.append("")
    out.extend(shared_blocks)
    out.extend(typed_blocks)
    for alias in aliases:
        out.append(f"pub type {alias} = GenericEventSubPayload;")
    out.append("")
    out.append("#[allow(clippy::large_enum_variant)]")
    out.append("#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]")
    out.append("pub enum KnownEventSubPayload {")
    for variant, description, payload_type in variants:
        out.append(f"    /// {description}")
        out.append(f"    {variant}({payload_type}),")
    out.append("}")
    out.append("")
    out.append("pub static ALL_SUBSCRIPTIONS: &[EventSubSubscriptionDefinition] = &[")
    out.extend(all_entries)
    out.append("];")
    out.append("")
    out.append("pub(crate) fn decode_known_payload(")
    out.append("    subscription_type: &str,")
    out.append("    version: Option<&str>,")
    out.append("    event: Option<&serde_json::Value>,")
    out.append("    source_timestamp: Option<OffsetDateTime>,")
    out.append(") -> Option<KnownEventSubPayload> {")
    out.append("    match (subscription_type, version) {")
    out.extend(match_arms)
    out.append("        _ => None,")
    out.append("    }")
    out.append("}")
    out.append("")
    out.append("/// Decodes an event payload by reference and stamps the envelope timestamp on it.")
    out.append("pub(super) fn decode_typed_event<T: serde::de::DeserializeOwned + super::HasSourceTimestamp>(")
    out.append("    event: Option<&serde_json::Value>,")
    out.append("    source_timestamp: Option<OffsetDateTime>,")
    out.append(") -> Option<T> {")
    out.append("    let mut payload: T = T::deserialize(event?).ok()?;")
    out.append("    payload.set_source_timestamp(source_timestamp);")
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
    eventsub_reference_html = pathlib.Path(args.eventsub_reference_html).read_text()
    warnings: list[str] = []
    generated_at = now_iso()
    helix = parse_helix(reference_html, warnings, generated_at=generated_at)
    validate_responses(helix, warnings)
    eventsub = parse_eventsub(eventsub_html, generated_at=generated_at)
    attach_eventsub_events(eventsub, eventsub_reference_html, warnings)
    validate_events(eventsub, warnings)
    auth = {
        "generated_at": generated_at,
        "source_urls": SOURCE_URLS,
        **AUTH_SURFACE,
    }
    summary = build_summary(helix, eventsub, auth, generated_at=generated_at)

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
    print(
        json.dumps(
            {
                "typed": sum(
                    1
                    for endpoint in helix["endpoints"]
                    if endpoint["response"] is not None and endpoint["response"]["fields"]
                ),
                "unit": sum(
                    1
                    for endpoint in helix["endpoints"]
                    if endpoint["response"] is not None
                    and not endpoint["response"]["fields"]
                ),
                "untyped": [
                    endpoint["id"]
                    for endpoint in helix["endpoints"]
                    if endpoint["response"] is None
                ],
                "eventsub_typed": summary["counts"]["eventsub_typed_events"],
                "eventsub_untyped": [
                    item["id"]
                    for item in eventsub["subscriptions"]
                    if item["event"] is None
                ],
                "warnings": warnings,
            },
            indent=2,
        ),
        file=sys.stderr,
    )
    return 0


def load_catalog_dir(path: pathlib.Path) -> dict[str, Any]:
    return {
        name: json.loads((path / f"{name}.json").read_text())
        for name in ("helix", "eventsub", "auth", "summary")
    }


def response_field_paths(response: dict[str, Any] | None) -> set[str]:
    paths: set[str] = set()

    def walk(fields: list[dict[str, Any]], prefix: str) -> None:
        for field in fields:
            path = f"{prefix}{field['name']}"
            paths.add(path)
            walk(field.get("children", []), f"{path}.")

    if response is not None:
        walk(response.get("fields") or [], "")
    return paths


def diff_catalogs(old: dict[str, Any], new: dict[str, Any]) -> tuple[str, int]:
    lines = ["# Twitch Surface Drift Report", ""]
    severity = 0

    def diff_named(kind: str, old_items: list[dict[str, Any]], new_items: list[dict[str, Any]]) -> None:
        nonlocal severity
        old_by_id = {item["id"]: item for item in old_items}
        new_by_id = {item["id"]: item for item in new_items}
        added = sorted(set(new_by_id) - set(old_by_id))
        removed = sorted(set(old_by_id) - set(new_by_id))
        changed = [
            item_id
            for item_id in sorted(set(old_by_id) & set(new_by_id))
            if old_by_id[item_id] != new_by_id[item_id]
        ]

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
                for shape_key in ("response", "event"):
                    if shape_key not in differing:
                        continue
                    old_paths = response_field_paths(before.get(shape_key))
                    new_paths = response_field_paths(after.get(shape_key))
                    for path in sorted(new_paths - old_paths):
                        lines.append(f"  - {shape_key} field added: `{path}`")
                    for path in sorted(old_paths - new_paths):
                        lines.append(f"  - {shape_key} field removed: `{path}`")
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
    build.add_argument("--eventsub-reference-html", required=True)
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
