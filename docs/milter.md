# Milter Protocol Support

This document describes the Milter protocol support in Renews NNTP server, which allows integration with external content filtering systems.

## Overview

The Milter (Mail Filter) protocol is an industry-standard interface originally developed for Sendmail to communicate with external filter programs. Renews implements a Milter client that can send news articles to external Milter servers for validation, spam filtering, content scanning, or other processing.

## Configuration

The Milter filter is configured by adding it to the filter pipeline:

```toml
[[filters]]
name = "MilterFilter"
address = "tcp://127.0.0.1:8888"     # Milter server address with protocol scheme
timeout_secs = 30                    # Connection timeout in seconds
```

## Configuration Options

### address
- **Type**: String
- **Required**: Yes
- **Description**: The address of the Milter server with protocol scheme
- **Supported Schemes**:
  - `tcp://` - Plain TCP connection
  - `tls://` - TLS-encrypted TCP connection  
  - `unix://` - Unix socket connection
- **Examples**: 
  - `"tcp://127.0.0.1:8888"` - Local Milter server over TCP
  - `"tls://milter.example.com:8889"` - Remote Milter server with TLS encryption
  - `"unix:///var/run/milter.sock"` - Unix socket connection

### timeout_secs
- **Type**: Integer
- **Default**: `30`
- **Description**: Connection timeout in seconds for connecting to the Milter server
- **Range**: 1-600 seconds

## Milter Protocol Implementation

The implementation follows the standard Milter protocol with the following message flow:

1. **Connect**: Sends connection information to the Milter server
2. **Headers**: Sends each article header individually
3. **End of Headers**: Signals the end of header transmission
4. **Body**: Sends the article body content
5. **End of Message**: Signals the end of message transmission
6. **Response Processing**: Processes the Milter server's decision
7. **Quit**: Closes the connection

## Response Handling

The Milter server can respond with various actions:

- **Accept** (`a`): Article is accepted and processing continues
- **Continue** (`c`): Continue processing (intermediate response)
- **Reject** (`r`): Article is rejected with an error
- **Discard** (`d`): Article is silently discarded
- **Temporary Failure** (`t`): Temporary failure, retry later

## Protocol Support

The Milter filter supports three connection types through URI schemes:

### TCP Connections
Plain TCP connections for basic communication:
```toml
address = "tcp://127.0.0.1:8888"
```

### TLS Connections  
TLS-encrypted connections for secure communication:
```toml
address = "tls://milter.example.com:8889"
```

When using TLS, the filter:
- Uses the system's certificate store for server validation
- Supports SNI (Server Name Indication) for hostname verification
- Compatible with standard TLS configurations

### Unix Socket Connections
Unix socket connections for local communication:
```toml
address = "unix:///var/run/milter.sock"
```

## Error Handling

The Milter filter handles various error conditions:

- **Connection failures**: Network connectivity issues
- **Protocol errors**: Invalid Milter protocol responses
- **TLS errors**: Certificate validation or encryption issues
- **Timeouts**: Connection or response timeouts
- **Article rejection**: When the Milter server rejects an article

## Filter Pipeline Integration

The MilterFilter integrates seamlessly with other filters in the pipeline:

```toml
[[filters]]
name = "HeaderFilter"      # Validate required headers first

[[filters]]
name = "SizeFilter"        # Check size limits

[[filters]]
name = "MilterFilter"      # External content filtering
address = "tcp://127.0.0.1:8888"
timeout_secs = 30

[[filters]]
name = "GroupExistenceFilter"  # Validate newsgroups exist

[[filters]]
name = "ModerationFilter"      # Handle moderation
```

## Example Configurations

### Basic TCP Milter

```toml
[[filters]]
name = "MilterFilter"
address = "tcp://127.0.0.1:8888"
timeout_secs = 30
```

### TLS Milter with Custom Timeout

```toml
[[filters]]
name = "MilterFilter"
address = "tls://secure-milter.example.com:8889"
timeout_secs = 60
```

### Unix Socket Milter

```toml
[[filters]]
name = "MilterFilter"
address = "unix:///var/run/milter.sock"
timeout_secs = 30
```

### Multiple Milter Servers

You can configure multiple Milter filters in the pipeline for different purposes:

```toml
# Spam filtering
[[filters]]
name = "MilterFilter"
address = "tcp://spamfilter.example.com:8888"
timeout_secs = 30

# Content scanning
[[filters]]
name = "MilterFilter"
address = "tls://contentfilter.example.com:8889"
timeout_secs = 45
```

## Milter Server Compatibility

This implementation should be compatible with most standard Milter servers, including:

- SpamAssassin with milter interface
- ClamAV milter
- OpenDKIM
- Custom Milter implementations

## Performance Considerations

- Milter filtering adds latency to article processing
- Configure appropriate timeouts based on your Milter server performance
- Consider using connection pooling in your Milter server for better performance
- TLS connections have additional overhead but provide security

## Troubleshooting

### Connection Issues
- Verify the Milter server is running and accessible
- Check firewall rules and network connectivity
- Verify the address and port configuration

### TLS Issues
- Ensure the Milter server supports TLS
- Check certificate validity and trust chain
- Verify hostname matches certificate

### Performance Issues
- Increase timeout values if the Milter server is slow
- Monitor Milter server performance and resources
- Consider load balancing multiple Milter servers

## Security Considerations

- Use TLS when connecting to remote Milter servers
- Ensure Milter servers are properly secured and updated
- Monitor Milter server logs for suspicious activity
- Consider network segmentation for Milter servers