# RustSpeak

## What is RustSpeak?

RustSpeak is a voice chat client and server designed to be as secure and decentralized as possible.

## Features

Among the security features it provides are:
- **impersonation protection**:
  - works even in the highly improbable case of certificate authorities being compromised
  - works even when certificate authorities are compromised and the server never saw the client before
- (configurable) **protection of servers** from malicious bot attacks through incremental proofs of work that require zero knowledge
- use of **4096 bit rsa** encryption keys for client ids
- use of **sha256** hashing algorithm  

Furthermore, it provides the following non-security related features:

- use of **QUIC**
- written in **Rust**
- supports **Windows, Linux and Mac** (This is currently untested as we are still in the testing phase)