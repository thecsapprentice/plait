# Error codes

Generated from `plait_proto::codes`; don't edit by hand (`PLAIT_WRITE_CODES=1 cargo test -p plait-proto codes`).

Every problem Plait can show has a code, `Q-AACC`: `AA` is the area and `CC` the case. In each area, 01–49 are refusals by the server and 50–99 are decided by the client. The desktop app shows codes in advanced mode; the CLI always prints them. A code never changes meaning, and a retired code is never reused.

## P-10xx: Connection and the server

| Code | Name | Meaning |
| --- | --- | --- |
| P-1001 | `NOT_FOUND` | What was asked for isn't there, or isn't shown to this login. |
| P-1002 | `TOO_LARGE` | The request was larger than the server takes. |
| P-1003 | `BAD_BODY` | The request wasn't JSON, or its body wasn't what the call takes. |
| P-1004 | `TOO_MANY` | Too many tries in a short time; wait and try again. |
| P-1005 | `SERVER_FAULT` | Something went wrong on the server. Its log says what. |
| P-1006 | `WRONG_MEDIA_TYPE` | The request's media type isn't one the call takes. |
| P-1007 | `FORM_NO_CHOICE` | A form on the web board was sent without one of its choices picked. |
| P-1008 | `FORM_NOT_CONFIRMED` | A delete on the web board was sent without ticking the box that confirms it. |
| P-1009 | `BAD_CURSOR` | The marker for the next page of a list isn't one the server gave out. |
| P-1050 | `UNREACHABLE` | The server couldn't be reached: offline, or the address is wrong. |
| P-1051 | `MALFORMED_REPLY` | The server's reply couldn't be read. |
| P-1052 | `REPLY_TOO_LARGE` | The server's reply was larger than a client accepts. |
| P-1053 | `UNEXPECTED_REPLY` | The server answered with a status this client doesn't expect. |
| P-1054 | `BAD_ADDRESS` | That isn't a server address, or it isn't https. |
| P-1055 | `SERVER_ALREADY_ADDED` | That server is already in this app's list. |
| P-1056 | `SERVER_NAME_BAD` | A server's name in this app must be 1 to 60 characters. |

## P-11xx: Sign-in and logins

| Code | Name | Meaning |
| --- | --- | --- |
| P-1101 | `SIGNED_OUT` | No login token, or one the server no longer accepts: sign in again. |
| P-1102 | `WRONG_LOGIN` | That login name and password don't match. |
| P-1103 | `REGISTRATION_CLOSED` | This server isn't taking new accounts right now. |
| P-1104 | `LOGIN_NAME_TAKEN` | That login name is already taken. |
| P-1150 | `NOT_SIGNED_IN` | This computer has no login token for the server: sign in. |

## P-12xx: Server rules

| Code | Name | Meaning |
| --- | --- | --- |
| P-1201 | `RULES_NOT_ACCEPTED` | The server rules have changed; read and accept them to keep writing. |
| P-1202 | `RULES_CHANGED` | The rules changed after they were read; read them again to accept. |
| P-1203 | `RULES_CHANGED_AT_SIGNUP` | The rules changed while the account was being made; read them again. |
| P-1204 | `RULES_REQUIRED_AT_SIGNUP` | This server asks new accounts to accept its rules first. |

## P-13xx: This computer's store and keychain

| Code | Name | Meaning |
| --- | --- | --- |
| P-1350 | `STORE_IO` | The store file couldn't be read or written. |
| P-1351 | `STORE_EXISTS` | A store already exists where a new one was to be made. |
| P-1352 | `STORE_IN_USE` | The store is open in another copy of Plait. |
| P-1353 | `NOT_A_STORE` | That file isn't a Plait store. |
| P-1354 | `STORE_TOO_NEW` | The store was made by a newer Plait: update this one. |
| P-1355 | `STORE_DAMAGED` | The store is damaged. |
| P-1356 | `WRONG_PASSPHRASE` | Wrong passphrase, or the keychain holds a different key. |
| P-1357 | `WRONG_UNLOCK` | The store is unlocked another way (passphrase or keychain). |
| P-1358 | `KEY_NOT_IN_KEYCHAIN` | The store's key isn't in this computer's keychain. |
| P-1359 | `KEYCHAIN_FAILED` | The keychain refused or failed. |
| P-1360 | `STORE_DATABASE` | The store's database failed. |
| P-1361 | `NO_KEYCHAIN` | This computer has no keychain for Plait's key: choose a passphrase. |
| P-1362 | `STORES_NOT_MOVED` | The stores couldn't be moved to that folder. |
| P-1363 | `PASSPHRASE_TOO_SHORT` | A store's passphrase must be at least 12 characters. |
| P-1364 | `STORE_BUSY` | The store is busy with something else; try again in a moment. |

## P-14xx: Personas and handles

| Code | Name | Meaning |
| --- | --- | --- |
| P-1401 | `PERSONA_UNKNOWN` | That persona isn't one of this login's. |
| P-1402 | `PERSONA_LIMIT` | This login has as many personas as the server allows. |
| P-1403 | `HANDLE_TAKEN` | That handle is taken or otherwise not available. |
| P-1404 | `HANDLE_STAFF_RESERVED` | That handle is kept for the server's staff. |
| P-1405 | `RESERVED_NONE_GIVEN` | No handles were entered to reserve. |
| P-1406 | `RESERVED_FROM_FILE` | That reserved handle comes from the server's reserved-handles file, so it can only be removed there. |
| P-1407 | `STAFF_ADMIN_ONLY` | Only administrators have a staff name or speak as staff. |
| P-1408 | `STAFF_NAME_TAKEN` | Another administrator already uses that staff name. |
| P-1409 | `STAFF_NAME_UNSET` | Set a staff name in your settings before speaking as staff. |
| P-1450 | `PERSONA_GONE` | That persona is gone: deleted, perhaps from another device. |

## P-15xx: Identity keys, pins and KeyPackages

| Code | Name | Meaning |
| --- | --- | --- |
| P-1501 | `KEYS_NOT_BASE64` | A key, certificate or other key data wasn't written as base64url. |
| P-1502 | `KEYS_REFUSED` | An identity key, device certificate or revocation didn't check out. |
| P-1503 | `KEYS_VERSION_OUT_OF_RANGE` | An identity key's version must be 1 or more, and not too large. |
| P-1504 | `KEYS_VERSION_NOT_HIGHER` | A replacement identity key needs a higher version than the current one. |
| P-1505 | `KEYS_IDENTITY_IN_USE` | That identity key is already used by another persona. |
| P-1506 | `KEYS_NO_IDENTITY` | The persona has no identity key yet; it needs one first. |
| P-1507 | `KEYS_PERSONA_HAS_NONE` | That persona has no keys, so it can't be invited to sessions. |
| P-1508 | `KEYPACKAGE_REFUSED` | A KeyPackage couldn't be read, or isn't one the server takes. |
| P-1509 | `KEYPACKAGE_POOL_FULL` | A device already has as many KeyPackages waiting as it may. |
| P-1550 | `KEYS_CHECK_FAILED` | A key check failed: something published doesn't verify. |
| P-1551 | `KEY_CHANGED` | Another writer's identity key changed since this computer pinned it. |
| P-1552 | `NO_KEYS_HERE` | This persona has no keys on this computer. |
| P-1553 | `NO_IDENTITY_KEY` | That writer has no identity key yet, so can't take part in sessions. |
| P-1554 | `BAD_CERTIFICATE` | A device certificate doesn't verify. |
| P-1555 | `MLS_FAILED` | The encryption library (MLS) refused or failed. |
| P-1556 | `KEY_PACKAGE_REFUSED` | A KeyPackage failed its checks. |
| P-1557 | `KEYS_ELSEWHERE` | The persona's identity key was made on another device: link this one. |

## P-16xx: Devices and linking

| Code | Name | Meaning |
| --- | --- | --- |
| P-1601 | `DEVICE_KEY_REGISTERED` | That device key is already registered. |
| P-1602 | `DEVICE_REVOKED_FOR_GOOD` | That device key was revoked, and revocation is permanent: make a new device key. |
| P-1603 | `DEVICE_ALREADY_CERTIFIED` | That device is already certified. |
| P-1604 | `DEVICE_REVOCATION_MISMATCH` | The revocation is for a different device. |
| P-1605 | `DEVICE_ALREADY_REVOKED` | That device is already revoked. |
| P-1606 | `DEVICE_REVOKED` | That device is revoked, so it can't be used. |
| P-1607 | `DEVICE_NEEDS_CERTIFYING` | That device needs certifying with the persona's current identity key. |
| P-1608 | `DEVICE_GROUP_NOT_PERSONAL` | Only a personal device group can be started this way. |
| P-1609 | `DEVICE_GROUP_BAD_ID` | A device group's id must be 16 to 64 bytes. |
| P-1610 | `DEVICE_GROUP_EXISTS` | This login already has a personal device group. |
| P-1650 | `DEVICES_CHECK_FAILED` | A check on the login's devices failed. |
| P-1651 | `LINK_EXPIRED` | The link expired before it was approved. |
| P-1652 | `LINK_NOT_ANSWERED` | No device approved the link in time. |
| P-1653 | `LINK_MISMATCH` | The new device's key doesn't match the code or words: don't link it. |
| P-1654 | `REMOVED_HERE` | This computer was removed from the login's devices. |
| P-1655 | `LINK_CODE_BAD` | That isn't a link code or six words from the new computer. |
| P-1656 | `DEVICE_NAME_BAD` | A device's name must be 1 to 40 characters on one line. |
| P-1657 | `FELL_BEHIND` | This computer was away too long to catch up with the login's devices. |

## P-20xx: Prompts

| Code | Name | Meaning |
| --- | --- | --- |
| P-2001 | `PROMPT_REMOVED` | An administrator removed this prompt, so it can't be changed. |

## P-21xx: Tags and tag types

| Code | Name | Meaning |
| --- | --- | --- |
| P-2101 | `TAG_TYPE_UNKNOWN` | There's no tag type by that name. |
| P-2102 | `TAG_TYPE_EXISTS` | A tag type by that name already exists. |
| P-2103 | `TAG_TYPE_MIN_UNREACHABLE` | A curated-only tag type asks for more tags than it has to pick from. |
| P-2104 | `TAG_TYPES_MIN_TOO_HIGH` | Together the tag types ask for more tags than a prompt can have. |
| P-2105 | `TAG_UNKNOWN` | There's no tag by that name. |
| P-2106 | `TAG_NEW_NOT_ALLOWED` | That tag isn't on this server, and writers can't add it: its type is curated, or the server doesn't take new tags. |
| P-2107 | `TAG_ALREADY_CURATED` | That tag is already a curated tag. |
| P-2108 | `TAG_NOT_AWAITING` | Only new tags from writers can be awaiting approval. |
| P-2109 | `TAG_SELF_ALIAS` | A tag can't be an alias of itself. |
| P-2110 | `TAG_SEARCH_TOO_LONG` | A tag search is too long. |
| P-2111 | `TAGS_WRONG_COUNT` | A prompt has too few or too many tags of some type. |

## P-22xx: Reports, notices and automated moderation

| Code | Name | Meaning |
| --- | --- | --- |
| P-2201 | `CONTENT_REJECTED` | A moderation script refused what was written; the message says why. |
| P-2202 | `REPORT_BAD_STATE` | Reports can be listed as open or all, nothing else. |
| P-2203 | `REPORT_RESOLVED` | That report has already been resolved. |
| P-2204 | `REPORT_COMMENT_GONE` | The reported comment was deleted by its author; discard the report. |
| P-2205 | `AUTOMOD_DATA_BAD_NAME` | A common data entry's name must be lowercase letters, digits and underscores, up to 64 characters. |
| P-2206 | `AUTOMOD_DATA_TOO_LARGE` | A common data entry is larger than the server allows. |
| P-2207 | `AUTOMOD_DATA_DESCRIPTION_TOO_LONG` | A common data entry's description is longer than 500 characters. |
| P-2208 | `AUTOMOD_DATA_STALE` | The common data entry was saved by someone else meanwhile; reload it. |
| P-2209 | `AUTOMOD_DATA_FULL` | All common data together would be larger than the server allows. |
| P-2210 | `AUTOMOD_DATA_IN_USE` | That common data entry is used by a moderation script. |
| P-2211 | `AUTOMOD_SCRIPT_BAD_NAME` | A moderation script's name must be 1 to 100 characters. |
| P-2212 | `AUTOMOD_SCRIPT_TOO_LARGE` | A moderation script is larger than the server allows. |
| P-2213 | `AUTOMOD_SCRIPT_INVALID` | A moderation script doesn't compile; the message says where. |
| P-2214 | `AUTOMOD_SCRIPT_MISSING_DATA` | A moderation script reads common data that doesn't exist yet. |
| P-2215 | `AUTOMOD_SCRIPT_FAILS_SAMPLE` | A moderation script failed on a sample post. |
| P-2216 | `AUTOMOD_SCRIPT_DATA_DELETED` | Common data a moderation script reads was deleted while it was saved. |
| P-2217 | `AUTOMOD_SCRIPT_NAME_TAKEN` | A moderation script by that name already exists. |
| P-2218 | `AUTOMOD_CHAIN_TOO_LONG` | A moderation chain has more scripts than it can run. |
| P-2219 | `AUTOMOD_CHAIN_REPEATS` | A moderation chain names the same script twice. |
| P-2220 | `AUTOMOD_CHAIN_UNKNOWN_SCRIPT` | A moderation chain names a script that doesn't exist. |

## P-23xx: Announcements and comments

| Code | Name | Meaning |
| --- | --- | --- |
| P-2301 | `COMMENTS_CLOSED` | Comments are closed on this announcement. |
| P-2302 | `COMMENT_TWO_SPEAKERS` | A comment speaks as a persona or as staff, not both. |
| P-2303 | `COMMENT_STAFF_NOT_REMOVABLE` | Comments from the administrators can't be removed, only deleted. |
| P-2304 | `COMMENT_ALREADY_REMOVED` | That comment has already been removed. |

## P-24xx: Administration and server settings

| Code | Name | Meaning |
| --- | --- | --- |
| P-2401 | `ADMIN_ONLY` | Only the server's administrators can do that. |
| P-2402 | `SETTINGS_LIMITS_NOT_NUMBERS` | The server's limits must be whole numbers. |

## P-30xx: Requests

| Code | Name | Meaning |
| --- | --- | --- |
| P-3001 | `REQUEST_NOT_BASE64` | Some part of a request wasn't written as base64url. |
| P-3002 | `REQUEST_BAD_PITCH` | A request's opening message (its pitch) couldn't be read. |
| P-3003 | `REQUEST_BAD_EXPIRY` | A request's expiry is outside the days the server allows. |
| P-3004 | `REQUEST_OWN_PROMPT` | That's your own prompt: you can't ask to join it. |
| P-3005 | `REQUEST_ALREADY_WAITING` | You already have a request waiting on this prompt. |
| P-3006 | `REQUEST_ALREADY_SESSION` | The request was accepted and is a session now. |
| P-3007 | `REQUEST_DECLINED` | The request was declined. |
| P-3008 | `REQUEST_EXPIRED` | The request expired. |
| P-3009 | `REQUEST_BAD_STATE` | Requests can be listed as pending or held, nothing else. |
| P-3050 | `REQUEST_CHECK_FAILED` | A request failed a check on this computer. |
| P-3051 | `PITCH_EMPTY` | The pitch is empty. |
| P-3052 | `PITCH_TOO_LONG` | The pitch is longer than requests allow. |
| P-3053 | `AUTHOR_HAS_NO_DEVICES` | The prompt's author has no devices that can take a request now. |
| P-3054 | `READ_IT_HERE_FIRST` | A request must be read on this computer before it's accepted here. |
| P-3055 | `REQUEST_NOT_SENT_HERE` | The request was sent before this computer was linked. |
| P-3056 | `REQUEST_SENT_FROM_HERE` | This computer sent the request, under another login. |
| P-3057 | `REQUEST_REFUSED` | The request failed its checks (a forged group or pitch): never shown. |

## P-31xx: Sessions and groups

| Code | Name | Meaning |
| --- | --- | --- |
| P-3101 | `GROUP_NOT_BASE64` | A session id or other group data wasn't written as base64url. |
| P-3102 | `NOT_A_WELCOME` | What was sent as a welcome to a group isn't one. |
| P-3103 | `GROUP_ID_TAKEN` | That group id is already taken. |
| P-3104 | `SESSION_ENDED` | The session has ended. |
| P-3105 | `SESSION_WELCOME_NOT_ALLOWED` | Welcomes can only be left for your own persona's devices in the session, or for a device asking to rejoin. |
| P-3106 | `SESSION_REJOIN_REFUSED` | A device's request to rejoin a session didn't check out. |
| P-3150 | `SESSION_CHECK_FAILED` | A session failed a check on this computer. |

## P-32xx: Messages and channels

| Code | Name | Meaning |
| --- | --- | --- |
| P-3201 | `MESSAGE_UNREADABLE` | A message couldn't be read as one the server takes. |
| P-3202 | `MESSAGE_WRONG_GROUP` | A message was sent to a different group than it was written for. |
| P-3203 | `MESSAGE_STALE_EPOCH` | The group moved on while the message was written; catch up and send again. |
| P-3204 | `MESSAGE_TOO_MANY_ATTACHMENTS` | A message has more attachments than the server allows. |

## P-33xx: Attachments and storage

| Code | Name | Meaning |
| --- | --- | --- |
| P-3301 | `ATTACHMENTS_OFF` | This server doesn't take attachments. |
| P-3302 | `ATTACHMENT_STORAGE_FULL` | Your attachment storage is full; it frees up as messages are read. |
| P-3303 | `ATTACHMENT_NOT_PADDED` | An attachment wasn't padded to one of the allowed sizes. |
| P-3304 | `ATTACHMENT_NOT_YOURS` | A message names attachments that aren't the sender's to attach there. |
| P-3350 | `ATTACHMENT_CHECK_FAILED` | An attachment failed a check on this computer. |
| P-3351 | `ATTACHMENT_ELSEWHERE` | That attachment is on another server (federation comes later). |
| P-3352 | `ATTACHMENT_TOO_LARGE` | A file is larger than this server takes, so it wasn't attached. |
| P-3353 | `ATTACHMENT_UNREADABLE` | A file couldn't be read on this computer, so it wasn't attached. |

## P-34xx: Backup and recovery

| Code | Name | Meaning |
| --- | --- | --- |
| P-3401 | `BACKUP_TOO_LARGE` | A backup is larger than this server keeps. |
| P-3402 | `BACKUP_EMPTY_PART` | A part of a backup was empty. |
| P-3403 | `BACKUP_INCOMPLETE` | Some parts of the backup never arrived. |
| P-3450 | `BACKUP_CHECK_FAILED` | A backup failed a check on this computer. |
| P-3451 | `BACKUP_DAMAGED` | A backup couldn't be decrypted or is damaged. |

## P-35xx: History and sync

| Code | Name | Meaning |
| --- | --- | --- |
| P-3501 | `SYNC_ACK_OUT_OF_RANGE` | A device marked a place as read that isn't in the session's log. |
| P-3502 | `SYNC_ACK_NOT_YOURS` | A device can only mark what it has read for its own persona's devices. |
| P-3503 | `SYNC_PRUNED` | The group's older messages were deleted, so this device can't follow it from where it is. |
| P-3550 | `HISTORY_CHECK_FAILED` | History failed a check on this computer. |
| P-3551 | `HISTORY_SYNC_FAILED` | History sync between devices failed a check. |

## P-90xx: Input checks shared by every area

| Code | Name | Meaning |
| --- | --- | --- |
| P-9001 | `BAD_LOGIN_NAME` | A login name must be 3 to 30 characters of a to z, 0 to 9 and _. |
| P-9002 | `BAD_PASSWORD` | A password is too short or too long. |
| P-9003 | `TEXT_TOO_LONG` | Text is too long, or holds control characters. |
| P-9004 | `REQUIRED_MISSING` | A required field is empty or too long. |
| P-9005 | `BAD_TAG_NAME` | A tag needs Latin letters or digits. |
| P-9006 | `BAD_TAG_FORM` | A tag must be written as type:name. |
| P-9007 | `BAD_TAG_TYPE_NAME` | A tag type's name must be letters, digits and hyphens. |
| P-9008 | `TOO_MANY_TAGS` | A prompt has more tags than it can carry. |
| P-9009 | `BAD_COLOUR` | A colour must be written as #rrggbb. |
| P-9010 | `BAD_RESERVED_HANDLE` | A reserved handle entry isn't a valid handle. |
| P-9011 | `BAD_CHANNEL` | A channel name must be a short slug. |
| P-9012 | `BAD_STAFF_NAME` | A staff name is empty, too long, or reserved. |
| P-9013 | `BAD_TAG_LIMITS` | A tag type's minimum and maximum don't fit together. |
| P-9050 | `CLI_USAGE` | The command was given something it can't use; its message says what. |

## P-99xx: Not yet given a code of its own

| Code | Name | Meaning |
| --- | --- | --- |
| P-9950 | `UNCLASSIFIED` | Something went wrong that has no code of its own yet. Advanced mode shows the detail; report it with that. |
