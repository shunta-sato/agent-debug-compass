pub fn print_help() {
    println!(
        "Usage: adc <status|doctor|capabilities|observe|agent-context|snapshot|capture|target|evidence|next-probe|fleet|recorder|arm|disarm|compare|list-runs|investigate|bundle>"
    );
}

pub fn print_help_for(args: &[String]) {
    match args {
        [] => print_help(),
        [cmd] if cmd == "-h" || cmd == "--help" => print_help(),
        [cmd, ..] if cmd == "observe" => println!(
            "Usage: adc observe --run-id <id> [--duration-ms N|--duration-sec N] [--interval-ms N] [--service-name NAME] [--log-file PATH] [--domain-events-file PATH] [--config-file PATH] [--otlp-file PATH] [--journald-jsonl-file PATH] [--perfetto-file PATH]"
        ),
        [cmd, ..] if cmd == "agent-context" => println!(
            "Usage: adc agent-context (--run-id <id>|--fleet-run-id <id>) [--service-name NAME] [--format markdown|json|openmetrics|otlp-json|journald-jsonl|perfetto-json]"
        ),
        [cmd, subcmd, ..] if cmd == "fleet" && subcmd == "enroll" => println!(
            "Usage: adc fleet enroll --target-id <id> --transport <local|mcp_stdio_over_ssh|managed_mcp> [--host HOST] [--port PORT] [--auth-token-file PATH] [--tls-ca-file PATH] [--tls-client-cert-file PATH] [--tls-client-key-file PATH] [--tag TAG]"
        ),
        [cmd, ..] if cmd == "fleet" => println!(
            "Usage: adc fleet <init|invite|enroll|enroll-kit|targets|discover|preflight|observe|snapshot|capture|evidence>"
        ),
        [cmd, ..] if cmd == "recorder" => println!(
            "Usage: adc recorder status\n       adc recorder mark --symptom TEXT [--incident-id ID] [--marker-id ID]\n       adc recorder incidents\n       adc recorder incident get --incident-id ID"
        ),
        [cmd, ..] if cmd == "investigate" => {
            println!(
                "Usage: adc investigate bug --symptom <text> [--run-id <id>|--fleet-run-id <id>|--duration-ms N] [--service-name NAME] [--inventory PATH]\n       adc investigate start (--run-id <id>|--fleet-run-id <id>) [--service-name NAME] [--inventory PATH] [--journal-lines N] [--format json|markdown]\n       adc investigate continue (--run-id <id>|--fleet-run-id <id>) --step-id <id> [--service-name NAME] [--ref-label LABEL] [--ref REF]\n       adc investigate session (--run-id <id>|--fleet-run-id <id>) --session-id <id>\n       adc investigate cleanup-sessions (--run-id <id>|--fleet-run-id <id>) [--max-sessions N] [--max-age-days N] [--dry-run|--execute]\n       adc investigate probe-result missing-capability --probe-plan-id ID --probe-id ID --missing-fact FACT [--hypothesis-id H]\n       adc investigate probe-result policy-denied --probe-plan-id ID --probe-id ID --reason TEXT [--hypothesis-id H]\n       adc investigate route-packs\n       adc investigate service <name> [--journal-lines N]\n       adc investigate ref --ref artifact://service_investigations/... [--limit N]"
            )
        }
        _ => print_help(),
    }
}
