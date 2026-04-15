# Deployment Report

Generated: 2026-03-23T21:46:52.918Z
Cluster: devnet (https://devnet.helius-rpc.com/?api-key=ad899a27-7037-4c79-a764-8162d25c576a)
Wallet: 8kPzhQoUPx7LYM18f9TzskW4ZgvGyq4jMPYZikqmHMH4
Source: deployments/devnet.json

## Summary

- **Total checks: 49/68**
- Programs: 7/7
- Mints: 3/3
- PDAs: 7/19
- Pools: 8/8
- Bonding Curves: 2/8
- Hook Accounts: 3/3
- ALT: 2/2
- Authority: 1/1
- Vault: 6/7
- Whitelist: 10/10

## Results

| Category | Check | Address | Status | Details |
|----------|-------|---------|--------|---------|
| Programs | amm | `J7JxmN...3bR5` | OK | BPF Loader OK, authority=8kPzhQoU... |
| Programs | transferHook | `5X5STg...jMqj` | OK | BPF Loader OK, authority=8kPzhQoU... |
| Programs | taxProgram | `FGgidf...kG4W` | OK | BPF Loader OK, authority=8kPzhQoU... |
| Programs | epochProgram | `E1u6fM...uA5h` | OK | BPF Loader OK, authority=8kPzhQoU... |
| Programs | staking | `DrFg87...Zv1H` | OK | BPF Loader OK, authority=8kPzhQoU... |
| Programs | conversionVault | `9SGsfh...h263` | OK | BPF Loader OK, authority=8kPzhQoU... |
| Programs | bondingCurve | `HT3vw2...qdzy` | OK | BPF Loader OK, authority=8kPzhQoU... |
| Mints | crime Mint | `DtbDMB...vAxR` | OK | decimals=6, supply=1000000000000000, T22=true, hookExt=true |
| Mints | fraud Mint | `78EhS3...zNtx` | OK | decimals=6, supply=1000000000000000, T22=true, hookExt=true |
| Mints | profit Mint | `Eaipvk...Np2a` | OK | decimals=6, supply=20000000000000, T22=true, hookExt=true |
| PDAs | adminConfig | `8NbR4x...m9YZ` | OK | Exists, owner=J7JxmNkz... OK |
| PDAs | swapAuthority | `DDLjeJ...aZqH` | FAIL | Not found |
| PDAs | taxAuthority | `FAdySh...ciNM` | FAIL | Not found |
| PDAs | epochState | `DR2Egt...j7Eg` | OK | Exists, owner=E1u6fM9P... OK |
| PDAs | stakePool | `HNNetq...gPDa` | OK | Exists, owner=DrFg87bR... OK |
| PDAs | escrowVault | `Qa1pJQ...pRBD` | OK | Exists, owner=DrFg87bR... OK |
| PDAs | stakeVault | `52VW6R...XjsM` | FAIL | Owner MISMATCH: got TokenzQd..., expected DrFg87bR... |
| PDAs | whitelistAuthority | `FRZqZN...NKCF` | OK | Exists, owner=5X5STgDb... OK |
| PDAs | carnageFund | `AvtbMe...MTXX` | OK | Exists, owner=E1u6fM9P... OK |
| PDAs | carnageSolVault | `BLhP2J...XuXH` | FAIL | Owner MISMATCH: got 11111111..., expected E1u6fM9P... |
| PDAs | carnageCrimeVault | `3aXU67...Vvs2` | FAIL | Owner MISMATCH: got TokenzQd..., expected E1u6fM9P... |
| PDAs | carnageFraudVault | `7iw8Pr...opuw` | FAIL | Owner MISMATCH: got TokenzQd..., expected E1u6fM9P... |
| PDAs | carnageSigner | `EQRP2H...gUco` | FAIL | Not found |
| PDAs | stakingAuthority | `HfQjWz...Pexy` | FAIL | Not found |
| PDAs | wsolIntermediary | `FFaeGg...8YjD` | FAIL | Owner MISMATCH: got Tokenkeg..., expected FGgidfhN... |
| PDAs | vaultConfig | `FcEJLN...Wm72` | OK | Exists, owner=9SGsfhxH... OK |
| PDAs | vaultCrime | `9QWrNP...J5Jk` | FAIL | Owner MISMATCH: got TokenzQd..., expected 9SGsfhxH... |
| PDAs | vaultFraud | `98Ke8C...qyuf` | FAIL | Owner MISMATCH: got TokenzQd..., expected 9SGsfhxH... |
| PDAs | vaultProfit | `2dD8yf...X7Dc` | FAIL | Owner MISMATCH: got TokenzQd..., expected 9SGsfhxH... |
| Pools | Pool crimeSol | `7Auii5...7rtt` | OK | reserveA=5063156590, reserveB=290000000000000 |
| Pools | crimeSol vaultA | `BjNeT6...cAvV` | OK | Exists, 165 bytes, 5065195870 lamports |
| Pools | crimeSol vaultB | `BYNNxo...SbuJ` | OK | Exists, 171 bytes, 2081040 lamports |
| Pools | crimeSol mint ordering | `7Auii5...7rtt` | OK | mintA=So111111... < mintB=DtbDMB2d... (canonical) |
| Pools | Pool fraudSol | `Fj555X...nbNe` | OK | reserveA=5097887044, reserveB=290000000000000 |
| Pools | fraudSol vaultA | `4vdvGD...jZVb` | OK | Exists, 165 bytes, 5099926324 lamports |
| Pools | fraudSol vaultB | `5Jkcjc...DobC` | OK | Exists, 171 bytes, 2081040 lamports |
| Pools | fraudSol mint ordering | `Fj555X...nbNe` | OK | mintA=So111111... < mintB=78EhS3i2... (canonical) |
| Bonding Curves | crime curveState | `CNuXrg...b38K` | OK | Exists with data |
| Bonding Curves | crime tokenVault | `CA9Yng...SrSx` | FAIL | Not found |
| Bonding Curves | crime solVault | `A8Bj1k...m9yZ` | FAIL | Not found |
| Bonding Curves | crime taxEscrow | `DA7cXV...dZAC` | FAIL | Not found |
| Bonding Curves | fraud curveState | `N4bhtJ...QHjo` | OK | Exists with data |
| Bonding Curves | fraud tokenVault | `8PpZkP...9YCD` | FAIL | Not found |
| Bonding Curves | fraud solVault | `HdhpCn...KSym` | FAIL | Not found |
| Bonding Curves | fraud taxEscrow | `AvKUWQ...RxU1` | FAIL | Not found |
| Hook Accounts | ExtraAccountMetaList (crime) | `38xHZM...y75u` | OK | Exists with data |
| Hook Accounts | ExtraAccountMetaList (fraud) | `5y6SuF...1sHT` | OK | Exists with data |
| Hook Accounts | ExtraAccountMetaList (profit) | `9nuTKX...iA73` | OK | Exists with data |
| ALT | Address Lookup Table | `FwAetE...bMFL` | OK | Exists, 1816 bytes |
| ALT | ALT address spot-check | `FwAetE...bMFL` | OK | 3/3 spot-checked addresses found |
| Authority | Deployer wallet match | `8kPzhQ...HMH4` | OK | Wallet matches deployment.json authority.deployer |
| Vault | VaultConfig | `FcEJLN...Wm72` | OK | Exists with data |
| Vault | VaultCrime | `9QWrNP...J5Jk` | OK | balance=250000000000000 |
| Vault | VaultFraud | `98Ke8C...qyuf` | OK | balance=250000000000000 |
| Vault | VaultProfit | `2dD8yf...X7Dc` | FAIL | balance=19999999000000 (expected >= 20000000000000) |
| Vault | crime MintAuthority Burned | `DtbDMB...vAxR` | OK | Mint authority burned (null) |
| Vault | fraud MintAuthority Burned | `78EhS3...zNtx` | OK | Mint authority burned (null) |
| Vault | profit MintAuthority Burned | `Eaipvk...Np2a` | OK | Mint authority burned (null) |
| Whitelist | Whitelist: crimeSol VaultA | `GxSKB4...mxb7` | OK | Whitelisted |
| Whitelist | Whitelist: crimeSol VaultB | `9hgwpR...t2Ao` | OK | Whitelisted |
| Whitelist | Whitelist: fraudSol VaultA | `EPYM5C...9Pa1` | OK | Whitelisted |
| Whitelist | Whitelist: fraudSol VaultB | `9R9PTK...YZjz` | OK | Whitelisted |
| Whitelist | Whitelist: StakeVault | `5si4sk...iNi7` | OK | Whitelisted |
| Whitelist | Whitelist: CarnageCrimeVault | `E4Crnt...Dha1` | OK | Whitelisted |
| Whitelist | Whitelist: CarnageFraudVault | `EtsiCA...YTy1` | OK | Whitelisted |
| Whitelist | Whitelist: VaultCrime | `2jUySB...Xux7` | OK | Whitelisted |
| Whitelist | Whitelist: VaultFraud | `7PLCLs...aP6z` | OK | Whitelisted |
| Whitelist | Whitelist: VaultProfit | `G4zt48...wpQc` | OK | Whitelisted |

## Transaction Log

See: /Users/mlbob/Projects/Dr Fraudsworth/scripts/deploy/deploy-log-20260323T214639Z.txt

## Deployment Config

Source: deployments/devnet.json
Schema Version: 1
Generated At: 2026-03-23T20:22:44.972Z
