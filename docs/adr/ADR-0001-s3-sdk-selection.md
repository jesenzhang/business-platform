# ADR-0001: S3 SDK Selection

## Status
Accepted

## Context
The object storage adapter requires a production-grade S3-compatible client
with proper AWS Signature V4 signing, presigned URLs, and MinIO compatibility.

## Decision
Use the official `aws-sdk-s3` crate (v1.x).

Rationale:
- Official AWS SDK with active maintenance
- Built-in Signature V4 signing
- Native presigned URL support
- Path-style addressing for MinIO
- Streaming body support
- Well-tested error types

## Consequences
- Increased compile time (~30s additional)
- Larger dependency tree
- SDK types confined to adapter; domain uses provider-neutral object metadata
  and byte-stream contracts
- Small-object `Bytes` helpers are bounded to 16 MiB and implemented on top of
  the streaming interface
