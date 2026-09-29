# Optional workshop profile

Use only when the user explicitly selects this workshop profile. These values came from the supplied draft; they have not been live-verified during authoring.

| Setting | Supplied value |
|---|---|
| Observer | `https://dap-102.deslicer.show` |
| Environment | `dap-102` |
| Target group | Exact name `all_in_one_servers_ag1` only |
| Portal | `https://ops.deslicer.show` |
| Portal route | `/dashboard/dap/plans/<external-plan-id>` |
| Historical event pin | `v1.6.0`; enforce only if the current event explicitly requires it |

Within this selected profile, do not switch groups or infer the portal from `dap-102`. Confirm the group through CLI read-only commands. Credentials must already be configured out of band.

The draft records team “Default Team” and labels “Approve change request → Run change → Run change request → Done.” Treat these as operator guidance to verify in the actual portal, not a CLI contract or instructions for the agent to click them.

The draft's Inventory Groups UI hang and validation-502 observations were event-specific. Use CLI group inspection; report validation failures as unavailable evidence. Neither observation establishes general product behavior.

There is no approval-probe exception in this profile. Human reviews and executes; the agent proposes and inspects.
