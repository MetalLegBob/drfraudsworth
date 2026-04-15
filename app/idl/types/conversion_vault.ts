/**
 * Program IDL in camelCase format in order to be used in JS/TS.
 *
 * Note that this is only a type helper and is not the actual IDL. The original
 * IDL can be found at `target/idl/conversion_vault.json`.
 */
export type ConversionVault = {
  "address": "5uawA6ehYTu69Ggvm3LSK84qFawPKxbWgfngwj15NRJ",
  "metadata": {
    "name": "conversionVault",
    "version": "0.1.0",
    "spec": "0.1.0",
    "description": "Dr Fraudsworth Conversion Vault - Fixed-rate 100:1 token conversions"
  },
  "instructions": [
    {
      "name": "convert",
      "docs": [
        "Convert tokens at fixed 100:1 rate.",
        "Supports 4 paths: CRIME->PROFIT, FRAUD->PROFIT, PROFIT->CRIME, PROFIT->FRAUD."
      ],
      "discriminator": [
        122,
        80,
        212,
        208,
        92,
        200,
        34,
        161
      ],
      "accounts": [
        {
          "name": "user",
          "docs": [
            "User performing the conversion."
          ],
          "signer": true
        },
        {
          "name": "vaultConfig",
          "docs": [
            "VaultConfig PDA — needed for vault token account authority."
          ],
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  118,
                  97,
                  117,
                  108,
                  116,
                  95,
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ]
              }
            ]
          }
        },
        {
          "name": "userInputAccount",
          "docs": [
            "User's input token account (source — user sends tokens here)."
          ],
          "writable": true
        },
        {
          "name": "userOutputAccount",
          "docs": [
            "User's output token account (destination — user receives tokens here)."
          ],
          "writable": true
        },
        {
          "name": "inputMint",
          "docs": [
            "Input mint (CRIME, FRAUD, or PROFIT)."
          ]
        },
        {
          "name": "outputMint",
          "docs": [
            "Output mint (CRIME, FRAUD, or PROFIT)."
          ]
        },
        {
          "name": "vaultInput",
          "docs": [
            "Vault's input token account (receives user's input tokens).",
            "Validated: correct mint + correct PDA authority."
          ],
          "writable": true
        },
        {
          "name": "vaultOutput",
          "docs": [
            "Vault's output token account (sends converted tokens to user).",
            "Validated: correct mint + correct PDA authority."
          ],
          "writable": true
        },
        {
          "name": "tokenProgram"
        }
      ],
      "args": [
        {
          "name": "amountIn",
          "type": "u64"
        }
      ]
    },
    {
      "name": "convertV2",
      "docs": [
        "Convert tokens at fixed 100:1 rate with on-chain balance reading and slippage protection.",
        "",
        "Three modes controlled by `amount_in` and `pre_balance`:",
        "- `amount_in > 0`: Exact mode — convert exactly `amount_in` tokens.",
        "- `amount_in == 0, pre_balance == 0`: Convert-all — convert entire balance.",
        "- `amount_in == 0, pre_balance > 0`: Delta mode — convert only the tokens",
        "deposited since `pre_balance` (e.g. by a preceding AMM swap in an atomic",
        "multi-hop route). User's pre-existing holdings are untouched."
      ],
      "discriminator": [
        2,
        169,
        12,
        141,
        64,
        38,
        20,
        20
      ],
      "accounts": [
        {
          "name": "user",
          "docs": [
            "User performing the conversion."
          ],
          "signer": true
        },
        {
          "name": "vaultConfig",
          "docs": [
            "VaultConfig PDA — needed for vault token account authority."
          ],
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  118,
                  97,
                  117,
                  108,
                  116,
                  95,
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ]
              }
            ]
          }
        },
        {
          "name": "userInputAccount",
          "docs": [
            "User's input token account (source — user sends tokens here)."
          ],
          "writable": true
        },
        {
          "name": "userOutputAccount",
          "docs": [
            "User's output token account (destination — user receives tokens here)."
          ],
          "writable": true
        },
        {
          "name": "inputMint",
          "docs": [
            "Input mint (CRIME, FRAUD, or PROFIT)."
          ]
        },
        {
          "name": "outputMint",
          "docs": [
            "Output mint (CRIME, FRAUD, or PROFIT)."
          ]
        },
        {
          "name": "vaultInput",
          "docs": [
            "Vault's input token account (receives user's input tokens).",
            "Validated: correct mint + correct PDA authority."
          ],
          "writable": true
        },
        {
          "name": "vaultOutput",
          "docs": [
            "Vault's output token account (sends converted tokens to user).",
            "Validated: correct mint + correct PDA authority."
          ],
          "writable": true
        },
        {
          "name": "tokenProgram"
        }
      ],
      "args": [
        {
          "name": "amountIn",
          "type": "u64"
        },
        {
          "name": "minimumOutput",
          "type": "u64"
        },
        {
          "name": "preBalance",
          "type": "u64"
        }
      ]
    },
    {
      "name": "initialize",
      "docs": [
        "One-shot vault initialization. Creates VaultConfig PDA and 3 token accounts.",
        "Any signer can call — no authority stored."
      ],
      "discriminator": [
        175,
        175,
        109,
        31,
        13,
        152,
        155,
        237
      ],
      "accounts": [
        {
          "name": "payer",
          "docs": [
            "The program's upgrade authority. Must sign to prove deployer identity."
          ],
          "writable": true,
          "signer": true
        },
        {
          "name": "vaultConfig",
          "docs": [
            "VaultConfig singleton PDA."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  118,
                  97,
                  117,
                  108,
                  116,
                  95,
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ]
              }
            ]
          }
        },
        {
          "name": "vaultCrime",
          "docs": [
            "Vault's CRIME token account — PDA-derived, authority = vault_config."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  118,
                  97,
                  117,
                  108,
                  116,
                  95,
                  99,
                  114,
                  105,
                  109,
                  101
                ]
              },
              {
                "kind": "account",
                "path": "vaultConfig"
              }
            ]
          }
        },
        {
          "name": "vaultFraud",
          "docs": [
            "Vault's FRAUD token account — PDA-derived, authority = vault_config."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  118,
                  97,
                  117,
                  108,
                  116,
                  95,
                  102,
                  114,
                  97,
                  117,
                  100
                ]
              },
              {
                "kind": "account",
                "path": "vaultConfig"
              }
            ]
          }
        },
        {
          "name": "vaultProfit",
          "docs": [
            "Vault's PROFIT token account — PDA-derived, authority = vault_config."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  118,
                  97,
                  117,
                  108,
                  116,
                  95,
                  112,
                  114,
                  111,
                  102,
                  105,
                  116
                ]
              },
              {
                "kind": "account",
                "path": "vaultConfig"
              }
            ]
          }
        },
        {
          "name": "crimeMint",
          "docs": [
            "CRIME mint (validated via constraint; bypassed in localnet)."
          ]
        },
        {
          "name": "fraudMint",
          "docs": [
            "FRAUD mint (validated via constraint; bypassed in localnet)."
          ]
        },
        {
          "name": "profitMint",
          "docs": [
            "PROFIT mint (validated via constraint; bypassed in localnet)."
          ]
        },
        {
          "name": "tokenProgram"
        },
        {
          "name": "program",
          "docs": [
            "The Conversion Vault program — used to look up its ProgramData address."
          ],
          "address": "5uawA6ehYTu69Ggvm3LSK84qFawPKxbWgfngwj15NRJ"
        },
        {
          "name": "programData",
          "docs": [
            "ProgramData account — upgrade_authority must match payer."
          ]
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": []
    }
  ],
  "accounts": [
    {
      "name": "vaultConfig",
      "discriminator": [
        99,
        86,
        43,
        216,
        184,
        102,
        119,
        77
      ]
    }
  ],
  "errors": [
    {
      "code": 6000,
      "name": "zeroAmount",
      "msg": "Input amount must be greater than zero"
    },
    {
      "code": 6001,
      "name": "outputTooSmall",
      "msg": "Output amount rounds to zero — input too small for conversion"
    },
    {
      "code": 6002,
      "name": "invalidMintPair",
      "msg": "Invalid mint pair — must be CRIME<->PROFIT or FRAUD<->PROFIT"
    },
    {
      "code": 6003,
      "name": "sameMint",
      "msg": "Input and output mints must be different"
    },
    {
      "code": 6004,
      "name": "invalidTokenProgram",
      "msg": "Invalid token program — must be Token-2022"
    },
    {
      "code": 6005,
      "name": "mathOverflow",
      "msg": "Overflow in conversion calculation"
    },
    {
      "code": 6006,
      "name": "slippageExceeded",
      "msg": "Output below minimum — slippage protection"
    },
    {
      "code": 6007,
      "name": "invalidOwner",
      "msg": "Input account not owned by signer"
    },
    {
      "code": 6008,
      "name": "deltaUnderflow",
      "msg": "Pre-balance exceeds current balance — stale or incorrect snapshot"
    }
  ],
  "types": [
    {
      "name": "vaultConfig",
      "docs": [
        "Global vault configuration PDA.",
        "Seeds: [\"vault_config\"]",
        "",
        "Minimal state — all conversion parameters are hardcoded constants.",
        "No authority stored. No conversion rate stored.",
        "Upgrade authority managed by Squads multisig on the program itself.",
        "",
        "In localnet mode, mint addresses are stored in state (not hardcoded)",
        "so integration tests with random mints can exercise the vault."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "bump",
            "docs": [
              "PDA bump seed for deterministic re-derivation."
            ],
            "type": "u8"
          }
        ]
      }
    }
  ]
};
