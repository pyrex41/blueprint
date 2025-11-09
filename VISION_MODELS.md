# Vision Model Configuration

The blueprint vectorization system supports multiple vision models for wall extraction and room detection.

## Supported Models

### OpenAI Models

| Model | Speed | Cost | Quality | Use Case |
|-------|-------|------|---------|----------|
| **gpt-4o-mini** | ⚡⚡⚡ Very Fast | 💰 Cheapest | ⭐⭐⭐ Good | **Default** - Best for production, 60-80% cheaper than GPT-4 |
| **gpt-4o** | ⚡⚡ Fast | 💰💰 Medium | ⭐⭐⭐⭐ Excellent | Balanced speed and quality |
| **gpt-5** | ⚡ Slow | 💰💰💰 Expensive | ⭐⭐⭐⭐⭐ Best | Highest quality, long reasoning time |

### Alternative Models (Future Support)

| Model | Provider | Notes |
|-------|----------|-------|
| grok-2-vision-1212 | xAI | $2 input / $10 output per 1M tokens |

## Configuration

### Environment Variable

Set the vision model globally:

```bash
export VISION_MODEL=gpt-4o-mini
```

### API Request

Override per request by including `vision_model` in the request payload:

```json
{
  "image": "base64_encoded_image...",
  "strategy": "hybrid_vision",
  "vision_model": "gpt-4o-mini"
}
```

### Default Behavior

- **Default model**: `gpt-4o-mini` (fast and cost-effective)
- **Fallback**: If `VISION_MODEL` is not set, defaults to `gpt-4o-mini`

## Performance Comparison

Based on typical blueprint processing:

| Model | Avg Response Time | Tokens Used | Estimated Cost per Request |
|-------|-------------------|-------------|----------------------------|
| gpt-4o-mini | ~3-5 seconds | ~1500 | $0.001 |
| gpt-4o | ~5-8 seconds | ~2000 | $0.005 |
| gpt-5 | ~15-60 seconds | ~3000-5000 | $0.030 |

## Timeout Configuration

- **HTTP Client Timeout**: 300 seconds (5 minutes)
- **API Call Timeout**: 180 seconds (3 minutes)
- Configured to handle GPT-5's extended reasoning time

## Switching Models

To test different models:

```bash
# Use fastest model (default)
export VISION_MODEL=gpt-4o-mini
cargo run --release --bin axum-backend

# Use balanced model
export VISION_MODEL=gpt-4o
cargo run --release --bin axum-backend

# Use highest quality model
export VISION_MODEL=gpt-5
cargo run --release --bin axum-backend
```

## Recommendations

- **Development**: `gpt-4o-mini` for fast iteration
- **Production**: `gpt-4o-mini` for cost efficiency, or `gpt-4o` for better accuracy
- **High Stakes**: `gpt-5` when accuracy is critical and cost is not a concern

## API Keys

All OpenAI models require `OPENAI_API_KEY` environment variable:

```bash
export OPENAI_API_KEY=sk-...
```

## Future Enhancements

- [ ] Add Grok vision model support
- [ ] Add Claude 3.5 vision support
- [ ] Implement model fallback chain
- [ ] Add response caching for identical requests
