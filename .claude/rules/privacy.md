# Privacy Rules
- Do not log secrets, vault passphrases, full prompts, transcripts, or legal matter text.
- Treat local legal text and PII as sensitive even when it stays on disk.
- Do not write .env or credential files unless the user explicitly asks.
- Keep generated harness state under .agent-harness/.
- Full transcript backups require ANNO_AGENT_HARNESS_BACKUP_TRANSCRIPTS=1.
