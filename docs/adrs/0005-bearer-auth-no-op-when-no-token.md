# ADR-005 — Bearer-auth middleware is a no-op when no token is configured

**Status:** Accepted (v0.4) · **Date:** 2026-05-15 · **Deciders:** anno team

## Context

The privacy gateway has 27 pre-existing server tests (and counting) that build `GatewayConfig::default()` and exercise routes without setting a bearer token. Adding hard-enforcement bearer-auth would break all 27.

Two ways to handle:

1. **Hard-enforce: no token in config → middleware returns 500.** Forces every test to set a token. Forces every dev / loopback deployment to set a token. Safest position.
2. **Soft-enforce: no token in config → middleware is a no-op.** Preserves dev/test ergonomics; deployers exposing the gateway beyond loopback MUST set a token (documented in the deployer guide §3).

Hard-enforcement matches what a textbook security review wants. Soft-enforcement matches how real teams use the gateway in development. The risk of the soft path is operator error — someone exposing a no-token gateway to a public IP.

## Decision

**Soft-enforce.** `auth::require_bearer` returns `Ok(next.run)` when `state.bearer_token() == None`. The deployer guide explicitly states that production deployments MUST set `ANNO_GATEWAY_BEARER_TOKEN`. The v0.4 readiness spec calls out G6 specifically: *"Operators who expose the gateway beyond loopback MUST configure a bearer token."*

This decision is bounded by §3 of the AI Act position paper (constraint C-5: cabinet does not put its name on the system) and §3.3 of the v0.4 deployer guide.

## Consequences

- 27 existing tests stay green without modification.
- Dev / loopback / `cargo run` workflows stay frictionless.
- A new operator who skips the deployer guide can deploy an unauthenticated gateway. **This is the cost.** Mitigated by: (a) default listen address is 127.0.0.1; (b) explicit "MUST set bearer token" callouts in deployer guide §2/§3 and DPIA R1; (c) the breach playbook detection path D-2 catches unauthorised access.
- A future v0.5+ hard-enforcement mode is open via a config flag (e.g. `require_bearer_token: bool`) that defaults to `false` in v0.4 but is required `true` for non-loopback `listen` addresses. Tracked as a v0.5 candidate.

## Reference

`crates/anno-privacy-gateway/src/auth.rs::require_bearer`, deployer guide §2 + §3, breach playbook §2 D-2.
