# AGENTS Instructions

This file provides guidance for AI coding assistants working with this project.

## Check task dependencies

Identify the skills, mise tools, and MCP tools this task needs. Check only those
dependencies. Missing unrelated tools must not block the task.

### Check Capa MCP

This project uses `toolExposure: search`. Capa exposes `search` and `call_tool`;
upstream tools are discovered on demand and may not appear by their own names.

1. Call Capa `search` with the task, server, or tool name.
2. If search returns a matching tool, Capa MCP is reachable and the tool is
   registered. Use `call_tool` to run the task's operation.
3. If search responds but finds no match, Capa MCP is reachable, but the tool
   was not found. Search by its configured name or description before reporting
   it unavailable.
4. If `search` is missing or fails before returning results, report that Capa
   MCP is unavailable in this agent session. This does not prove the Capa
   service is stopped.
5. If `call_tool` fails, report the tool and exact error. Do not claim the
   server is down unless the error confirms that.

Do not probe the local Capa URL with shell `curl`; the shell may not share the
agent's network. Continue work that does not need the unavailable capability.
Recommend `mise trust`, `mise install`, or `mise run prepare` only when setup is
the confirmed cause of a required dependency being unavailable.