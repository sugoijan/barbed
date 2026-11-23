#!/usr/bin/env python3
from __future__ import annotations

import argparse
import collections
import datetime as dt
import html
import json
import pathlib
import re
import sys
import urllib.request
from typing import Any


DOCS_ORIGIN = "https://dev.twitch.tv"
# Reference name (also the directory under `--docs-dir`) -> docs path of its
# index page. Each index links the resource pages, which are fetched as their
# markdown copies (`<page>.md`).
REFERENCE_PATHS = {
    "helix": "/docs/api/reference",
    "eventsub": "/docs/eventsub/reference",
}
FETCH_USER_AGENT = "Mozilla/5.0 (compatible; docs-catalog-sync)"

SOURCE_URLS = {
    "helix_reference": DOCS_ORIGIN + REFERENCE_PATHS["helix"],
    "eventsub_reference": DOCS_ORIGIN + REFERENCE_PATHS["eventsub"],
    "oauth_tokens": "https://dev.twitch.tv/docs/authentication/getting-tokens-oauth",
    "oauth_oidc": "https://dev.twitch.tv/docs/authentication/getting-tokens-oidc",
    "oauth_validate": "https://dev.twitch.tv/docs/authentication/validate-tokens",
    "oauth_revoke": "https://dev.twitch.tv/docs/authentication/revoke-tokens",
    "eventsub_webhooks": "https://dev.twitch.tv/docs/eventsub/handling-webhook-events/",
    "changelog": "https://dev.twitch.tv/docs/change-log",
    "product_lifecycle": "https://dev.twitch.tv/docs/change-log/product-lifecycle",
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
    if "beta-pill" in lowered or ">beta<" in lowered:
        return "beta"
    if "new-pill" in lowered or ">new<" in lowered:
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


# Reference index pages: one <section> per resource, one <dt>/<dd> pair per
# entry. The entry link carries the resource page and the entry's anchor on it;
# stability pills only exist here, not in the markdown copies.
INDEX_SECTION_RE = re.compile(
    r'<section class="ref-index-section">\s*<h2[^>]*>(?P<group>.*?)</h2>(?P<body>.*?)</section>',
    re.S,
)
INDEX_ENTRY_RE = re.compile(
    r'<dt class="ref-index-endpoint-name">\s*'
    r'<a href="[^"#]*/(?P<page>[a-z0-9-]+)#(?P<slug>[^"]+)">(?P<name>.*?)</a>\s*</dt>'
    r"\s*<dd[^>]*>(?P<summary>.*?)</dd>",
    re.S,
)
INDEX_PILL_RE = re.compile(r'<span class="ref-[a-z]+-pill">.*?</span>', re.S)


def parse_reference_index(index_html: str, kind: str) -> list[dict[str, str]]:
    entries = []
    for section in INDEX_SECTION_RE.finditer(index_html):
        group = clean_html(section.group("group"))
        for entry in INDEX_ENTRY_RE.finditer(section.group("body")):
            summary_html = entry.group("summary")
            entries.append(
                {
                    "group": group,
                    "page": entry.group("page"),
                    "slug": html.unescape(entry.group("slug")),
                    "name": clean_html(entry.group("name")),
                    "description": clean_html(INDEX_PILL_RE.sub("", summary_html)),
                    "stability": classify_stability(summary_html),
                }
            )
    if not entries:
        raise RuntimeError(f"failed to locate the {kind} reference index entries")
    return entries


MD_HEADING_RE = re.compile(r"^#+ (?P<title>.*?)(?: \{#(?P<slug>[^}]+)\})?\s*$")
MD_ESCAPE_RE = re.compile(r"\\([\\`*_{}\[\]()#+\-.!<>|~])")
MD_HIDDEN_RE = re.compile(r"\x00([0-9a-f]{2})")
MD_LINK_RE = re.compile(r"!?\[([^\]]*)\]\([^)]*\)")
MD_STRONG_RE = re.compile(r"\*\*(.+?)\*\*")
MD_EMPHASIS_RES = (
    re.compile(r"(?<![\w*])\*(?!\s)(.+?)(?<!\s)\*(?![\w*])"),
    re.compile(r"(?<!\w)_(?!\s)(.+?)(?<!\s)_(?!\w)"),
)
MD_LIST_MARKER_RE = re.compile(r"^[-*>] +")


def markdown_sections(
    lines: list[str], level: int
) -> list[tuple[str, str | None, list[str]]]:
    """Splits lines at the headings of one level into (title, anchor, body),
    ignoring anything inside fenced code."""
    prefix = "#" * level + " "
    sections: list[tuple[str, str | None, list[str]]] = []
    body: list[str] | None = None
    fenced = False
    for line in lines:
        if line.lstrip().startswith("```"):
            fenced = not fenced
        elif not fenced and line.startswith(prefix):
            heading = MD_HEADING_RE.match(line)
            assert heading is not None
            body = []
            sections.append((heading.group("title"), heading.group("slug"), body))
            continue
        if body is not None:
            body.append(line)
    return sections


def markdown_text(raw: str) -> str:
    """Reduces inline markdown to the plain text the docs render."""
    text = MD_ESCAPE_RE.sub(lambda match: f"\x00{ord(match.group(1)):02x}", raw)
    text = MD_LINK_RE.sub(r"\1", text)
    text = MD_STRONG_RE.sub(r"\1", text)
    for emphasis_re in MD_EMPHASIS_RES:
        text = emphasis_re.sub(r"\1", text)
    text = text.replace("`", "")
    text = MD_HIDDEN_RE.sub(lambda match: chr(int(match.group(1), 16)), text)
    return html.unescape(text)


def markdown_lines_text(lines: list[str], separator: str) -> str:
    """Plain text of a prose block: one entry per non-empty line, list and
    quote markers dropped."""
    cleaned = (
        re.sub(r"\s+", " ", markdown_text(MD_LIST_MARKER_RE.sub("", line.strip()))).strip()
        for line in lines
    )
    return separator.join(line for line in cleaned if line)


class ReferencePages:
    """The markdown resource pages of one reference, split lazily into
    entry anchor -> {subsection title -> lines}."""

    def __init__(self, directory: pathlib.Path) -> None:
        self.directory = directory
        self.pages: dict[str, dict[str, dict[str, list[str]]]] = {}

    def entry(self, page: str, slug: str) -> dict[str, list[str]]:
        if page not in self.pages:
            path = self.directory / f"{page}.md"
            lines = path.read_text(encoding="utf-8").splitlines()
            self.pages[page] = {
                anchor: {title: body for title, _, body in markdown_sections(entry, 3)}
                for _, anchor, entry in markdown_sections(lines, 2)
                if anchor is not None
            }
        try:
            return self.pages[page][slug]
        except KeyError:
            raise RuntimeError(
                f"{self.directory.name}/{page}.md has no `{slug}` entry"
            ) from None


# Documented types that cannot have nested child fields.
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

# Objects the EventSub reference repeats inline in several payloads, emitted
# once as `Shared*` structs. Keyed by section name (the field name, hyphenated):
# (struct name, documented as an array, field names). A payload field joins a
# section when its name, array-ness, and child names all match; when the docs
# change one of these shapes the section stops matching (with a warning) until
# the names here are updated. Array sections name the element struct in the
# singular.
SHARED_SECTIONS = {
    "choices": ("SharedChoice", True, ("id", "title", "bits_votes", "channel_points_votes", "votes")),
    "outcomes": ("SharedOutcome", True, ("id", "title", "color", "users", "channel_points", "top_predictors")),
    "top-predictors": ("SharedTopPredictor", True, ("user_id", "user_login", "user_name", "channel_points_won", "channel_points_used")),
    "emotes": ("SharedEmote", True, ("begin", "end", "id")),
    "reward": ("SharedReward", False, ("id", "title", "cost", "prompt")),
    "image": ("SharedImage", False, ("url_1x", "url_2x", "url_4x")),
    "message": ("SharedMessage", False, ("text", "emotes")),
    "max-per-stream": ("SharedMaxPerStream", False, ("is_enabled", "value")),
    "max-per-user-per-stream": ("SharedMaxPerUserPerStream", False, ("is_enabled", "value")),
    "global-cooldown": ("SharedGlobalCooldown", False, ("is_enabled", "seconds")),
    "bits-voting": ("SharedBitsVoting", False, ("is_enabled", "amount_per_vote")),
    "channel-points-voting": ("SharedChannelPointsVoting", False, ("is_enabled", "amount_per_vote")),
    "custom-power-up": ("SharedCustomPowerUp", False, ("id", "title", "bits", "prompt")),
    "product": ("SharedProduct", False, ("name", "bits", "sku", "in_development")),
}
# Fields that use a shared section under another name.
SHARED_FIELD_ALIASES = {"default_image": "image"}


def shared_field_slug(name: str) -> str:
    """The `SHARED_SECTIONS` key a payload field of this name would use."""
    return SHARED_FIELD_ALIASES.get(name, name).replace("_", "-")


def normalize_eventsub_type(raw: str) -> str:
    stripped = raw.strip()
    lowered = stripped.lower()
    if lowered in EVENTSUB_TYPE_ALIASES:
        return EVENTSUB_TYPE_ALIASES[lowered]
    if lowered in EVENTSUB_PRIMITIVE_TYPES or lowered in {"object", "object[]"}:
        return lowered
    return stripped


# A documented field is one list item: the dotted path from the top of the
# body, an em dash, the type, optional flags, then the description.
FIELD_LINE_RE = re.compile(r"^(?P<indent> *)- `(?P<path>[^`]+)` — (?P<rest>.*)$")
FIELD_REST_RE = re.compile(
    r"^(?P<type>[A-Za-z0-9_\[\]]+)(?P<flags>(?:, [A-Za-z*]+)*)"
    r"(?:\.(?: (?P<description>.*))?)?$"
)


def parse_field_list(
    lines: list[str],
    context_id: str,
    warnings: list[str],
    *,
    normalize: Any = None,
) -> list[dict[str, Any]] | None:
    """Parses a markdown field list into the nested catalog field shape.

    Children sit two spaces deeper than their parent and repeat its path as a
    prefix; a list item that fails either test (value lists, error tables) is
    part of the enclosing field's description.
    """
    fields: list[dict[str, Any]] = []
    # (path, field, description parts) along the current ancestor chain.
    stack: list[tuple[str, dict[str, Any], list[str]]] = []
    described: list[tuple[dict[str, Any], list[str]]] = []
    for line in lines:
        match = FIELD_LINE_RE.match(line)
        indent = len(match.group("indent")) if match else 0
        depth = indent // 2
        is_field = (
            match is not None
            and indent % 2 == 0
            and depth <= len(stack)
            and (depth == 0 or match.group("path").startswith(stack[depth - 1][0] + "."))
        )
        if not is_field:
            if stack:
                stack[-1][2].append(line)
            continue
        path = match.group("path")
        rest = FIELD_REST_RE.match(match.group("rest"))
        if rest is None:
            warnings.append(f"{context_id}: unparsed field line for `{path}`")
            continue
        del stack[depth:]
        type_str = rest.group("type")
        if normalize is not None:
            type_str = normalize(type_str)
        field: dict[str, Any] = {
            "name": path[len(stack[-1][0]) + 1 :] if stack else path,
            "type": type_str,
            "description": "",
        }
        if "Nullable" in rest.group("flags"):
            field["nullable"] = True
        if stack:
            parent = stack[-1][1]
            parent_type = parent["type"]
            # The docs occasionally type a parent as a scalar; the nesting is
            # explicit, so the parent is what gets corrected.
            if not response_type_can_nest(parent_type) or parent_type.lower() == "array":
                is_array = parent_type.endswith("[]") or parent_type.lower() == "array"
                retyped = "Object[]" if is_array else "Object"
                if normalize is not None:
                    retyped = normalize(retyped)
                warnings.append(
                    f"{context_id}: retyped scalar parent `{parent['name']}` "
                    f"({parent_type}) to {retyped}"
                )
                parent["type"] = retyped
            parent.setdefault("children", []).append(field)
        else:
            fields.append(field)
        parts = [rest.group("description") or ""]
        stack.append((path, field, parts))
        described.append((field, parts))
    for field, parts in described:
        field["description"] = markdown_lines_text(parts, " ")
    return fields or None


def parse_expected_status(lines: list[str]) -> int:
    match = re.search(r"\b([1-5]\d{2})\b", markdown_lines_text(lines, "\n"))
    return int(match.group(1)) if match else 200


HELIX_ENDPOINT_RE = re.compile(
    r"`(?P<method>GET|POST|PUT|PATCH|DELETE) https://api\.twitch\.tv/helix(?P<path>/[^`\s]*)`"
)
PAGINATION_PARAMETERS = {"after", "before"}


def parse_helix(
    docs_dir: pathlib.Path, warnings: list[str], *, generated_at: str
) -> dict[str, Any]:
    directory = docs_dir / "helix"
    index_html = (directory / "index.html").read_text(encoding="utf-8")
    pages = ReferencePages(directory)

    endpoints = []
    groups: dict[str, int] = collections.Counter()
    for entry in parse_reference_index(index_html, "helix"):
        group = entry["group"]
        endpoint_id = snake_case(f"{group}_{entry['name']}")
        sections = pages.entry(entry["page"], entry["slug"])
        method_match = HELIX_ENDPOINT_RE.search("\n".join(sections.get("Endpoint", [])))
        if method_match is None:
            raise RuntimeError(f"{endpoint_id}: no method and URL under `Endpoint`")
        path = method_match.group("path")
        auth_text = markdown_lines_text(sections.get("Authorization", []), "\n")
        query = parse_field_list(sections.get("Query Parameters", []), endpoint_id, warnings)
        expected_status = parse_expected_status(sections.get("Response Codes", []))
        response_fields = parse_field_list(
            sections.get("Response Body", []), endpoint_id, warnings
        )
        if response_fields is not None:
            response = {"fields": response_fields}
        elif expected_status == 204:
            response = {"fields": []}
        else:
            response = None
        # Cursor pagination shows up as an `after`/`before` query parameter or
        # as a top-level `pagination` object in the response.
        supports_pagination = any(
            field["name"] in PAGINATION_PARAMETERS for field in query or []
        ) or any(field["name"] == "pagination" for field in response_fields or [])
        endpoints.append(
            {
                "id": endpoint_id,
                "group": group,
                "name": entry["name"],
                "page": entry["page"],
                "slug": entry["slug"],
                "description": entry["description"],
                "stability": entry["stability"],
                "method": method_match.group("method"),
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


SUBSCRIPTION_TYPE_RE = re.compile(r"^- Type: `(?P<value>[^`]+)`", re.M)
SUBSCRIPTION_VERSION_RE = re.compile(r"^- Version: `(?P<value>[^`]+)`", re.M)
# The payload's top-level `event` row; batched subscriptions call it `events`.
EVENT_PAYLOAD_FIELDS = {"event", "events"}


def parse_eventsub(
    docs_dir: pathlib.Path, warnings: list[str], *, generated_at: str
) -> dict[str, Any]:
    directory = docs_dir / "eventsub"
    index_html = (directory / "index.html").read_text(encoding="utf-8")
    pages = ReferencePages(directory)

    subscriptions = []
    for entry in parse_reference_index(index_html, "eventsub"):
        sections = pages.entry(entry["page"], entry["slug"])
        subscription = "\n".join(sections.get("Subscription", []))
        type_match = SUBSCRIPTION_TYPE_RE.search(subscription)
        version_match = SUBSCRIPTION_VERSION_RE.search(subscription)
        if type_match is None or version_match is None:
            raise RuntimeError(
                f"{entry['page']}#{entry['slug']}: no type and version under `Subscription`"
            )
        subscription_type = type_match.group("value")
        version = version_match.group("value")
        item_id = snake_case(f"{subscription_type}_{version}")
        payload = parse_field_list(
            sections.get("Event Payload", []),
            item_id,
            warnings,
            normalize=normalize_eventsub_type,
        )
        event_fields = next(
            (
                field.get("children")
                for field in payload or []
                if field["name"] in EVENT_PAYLOAD_FIELDS
            ),
            None,
        )
        if not event_fields:
            warnings.append(f"{item_id}: no event fields documented")
        subscriptions.append(
            {
                "id": item_id,
                "name": entry["name"],
                "page": entry["page"],
                "slug": entry["slug"],
                "subscription_type": subscription_type,
                "version": version,
                "description": entry["description"],
                "stability": entry["stability"],
                "event": {"fields": event_fields} if event_fields else None,
            }
        )

    eventsub = {
        "generated_at": generated_at,
        "source": SOURCE_URLS["eventsub_reference"],
        "subscriptions": subscriptions,
    }
    eventsub["shared_objects"] = share_event_objects(subscriptions, warnings)
    return eventsub


def shape_signature(fields: list[dict[str, Any]]) -> tuple[Any, ...]:
    return tuple(
        (
            field["name"],
            field["type"],
            field.get("ref"),
            shape_signature(field.get("children", [])),
        )
        for field in fields
    )


def shared_slug_order() -> list[str]:
    """Shared sections ordered so that nested ones come first."""
    ordered: list[str] = []

    def visit(slug: str) -> None:
        if slug in ordered:
            return
        for name in SHARED_SECTIONS[slug][2]:
            inner = shared_field_slug(name)
            if inner in SHARED_SECTIONS and inner != slug:
                visit(inner)
        ordered.append(slug)

    for slug in sorted(SHARED_SECTIONS):
        visit(slug)
    return ordered


def share_event_objects(
    subscriptions: list[dict[str, Any]], warnings: list[str]
) -> dict[str, Any]:
    """Replaces the inline copies of each `SHARED_SECTIONS` object with a
    `ref` and returns the shared definitions, keyed by section."""
    shared: dict[str, Any] = {}

    def walk(fields: list[dict[str, Any]]) -> Any:
        for field in fields:
            yield field
            yield from walk(field.get("children", []))

    def documented_fields() -> Any:
        for item in subscriptions:
            if item["event"] is not None:
                yield from walk(item["event"]["fields"])

    for slug in shared_slug_order():
        _, is_array, names = SHARED_SECTIONS[slug]
        expected_type = "object[]" if is_array else "object"
        matches = [
            field
            for field in documented_fields()
            if shared_field_slug(field["name"]) == slug
            and field["type"] == expected_type
            and sorted(child["name"] for child in field.get("children", [])) == sorted(names)
        ]
        if not matches:
            warnings.append(
                f"shared object `{slug}` matches no documented field; "
                "update SHARED_SECTIONS"
            )
            continue
        signatures = collections.Counter(
            shape_signature(field["children"]) for field in matches
        )
        signature = signatures.most_common(1)[0][0]
        for field in matches:
            if shape_signature(field["children"]) != signature:
                continue
            children = field.pop("children")
            field["ref"] = slug
            shared.setdefault(slug, {"fields": children})
        if len(signatures) > 1:
            warnings.append(
                f"shared object `{slug}`: {len(matches) - signatures[signature]} "
                "field(s) with the same names differ in shape and stay inline"
            )
    return dict(sorted(shared.items()))


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
        for attr in field_attr(field, rename):
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


def helix_field_attr(field: dict[str, Any], rename: str | None) -> list[str]:
    # `null` decodes as the default value, as for EventSub payloads. The docs
    # flag only some of the fields they describe as null (`nullable` in the
    # catalog), so every field tolerates it rather than failing the response.
    args = ["default", 'deserialize_with = "crate::eventsub::null_default"']
    if rename is not None:
        args.append(f"rename = {rust_string(rename)}")
    return [f"#[serde({', '.join(args)})]"]


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


def eventsub_field_attr(field: dict[str, Any], rename: str | None) -> list[str]:
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
    struct_name, is_array, _ = section
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


def fetch_text(url: str) -> str:
    request = urllib.request.Request(url, headers={"User-Agent": FETCH_USER_AGENT})
    with urllib.request.urlopen(request, timeout=60) as response:
        return response.read().decode("utf-8")


def fetch_command(args: argparse.Namespace) -> int:
    """Downloads each reference index and the markdown copy of every resource
    page it links into `<docs-dir>/<reference>/`."""
    docs_dir = pathlib.Path(args.docs_dir)
    for kind, docs_path in REFERENCE_PATHS.items():
        directory = docs_dir / kind
        directory.mkdir(parents=True, exist_ok=True)
        index_html = fetch_text(DOCS_ORIGIN + docs_path)
        (directory / "index.html").write_text(index_html, encoding="utf-8")
        pages = sorted({entry["page"] for entry in parse_reference_index(index_html, kind)})
        for page in pages:
            markdown = fetch_text(f"{DOCS_ORIGIN}{docs_path}/{page}.md")
            (directory / f"{page}.md").write_text(markdown, encoding="utf-8")
        print(f"{kind}: index and {len(pages)} resource pages -> {directory}")
    return 0


def build_command(args: argparse.Namespace) -> int:
    docs_dir = pathlib.Path(args.docs_dir)
    warnings: list[str] = []
    generated_at = now_iso()
    helix = parse_helix(docs_dir, warnings, generated_at=generated_at)
    validate_responses(helix, warnings)
    eventsub = parse_eventsub(docs_dir, warnings, generated_at=generated_at)
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

    fetch = subparsers.add_parser("fetch", help="Download the Twitch reference docs.")
    fetch.add_argument("--docs-dir", required=True)
    fetch.set_defaults(func=fetch_command)

    build = subparsers.add_parser("build", help="Build catalog JSON and generated Rust registries.")
    build.add_argument("--docs-dir", required=True, help="Directory written by `fetch`.")
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
