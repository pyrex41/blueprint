# Deployment Guide for Fly.io

This guide covers deploying the Axum backend to Fly.io.

## Prerequisites

1. Install the Fly.io CLI:
   ```bash
   curl -L https://fly.io/install.sh | sh
   ```

2. Sign up or log in to Fly.io:
   ```bash
   fly auth signup  # or fly auth login
   ```

3. **Important**: Make sure you have a payment method added to Fly.io (even for free tier). Some regions require this.

## Initial Deployment

1. **Create the Fly.io app** (only needed once):
   ```bash
   fly launch --no-deploy
   ```
   
   This will:
   - Read your `fly.toml` configuration
   - Create an app on Fly.io
   - Set up the app name (you can customize this)

2. **Set your OpenAI API Key as a secret**:
   ```bash
   fly secrets set OPENAI_API_KEY=your-actual-api-key-here
   ```

3. **Deploy the application**:
   ```bash
   fly deploy
   ```
   
   This will:
   - Build your Docker image using cargo-chef for fast builds
   - Push it to Fly.io's registry
   - Deploy it to your configured region

## Updating the Deployment

To deploy updates:
```bash
fly deploy
```

## Monitoring

- **View logs**:
  ```bash
  fly logs
  ```

- **Check status**:
  ```bash
  fly status
  ```

- **Open the app**:
  ```bash
  fly open
  ```

- **Check health**:
  ```bash
  curl https://your-app-name.fly.dev/health
  ```

## Configuration

### Environment Variables

Set additional environment variables:
```bash
fly secrets set VARIABLE_NAME=value
```

View current secrets:
```bash
fly secrets list
```

### Scaling

**Scale memory/CPU**:
```bash
fly scale vm shared-cpu-2x --memory 2048
```

**Scale instances**:
```bash
fly scale count 2
```

**Auto-scaling** (already configured in fly.toml):
- `min_machines_running = 0` - scales to zero when idle
- `auto_start_machines = true` - starts automatically on request
- `auto_stop_machines = 'stop'` - stops when idle

### Regions

Deploy to multiple regions:
```bash
fly regions add iad lax fra
```

## Troubleshooting

1. **Check logs for errors**:
   ```bash
   fly logs --app your-app-name
   ```

2. **SSH into the machine**:
   ```bash
   fly ssh console
   ```

3. **View machine status**:
   ```bash
   fly machine list
   ```

4. **Restart the app**:
   ```bash
   fly machine restart
   ```

## API Endpoints

Once deployed, your backend will be available at:
- `https://your-app-name.fly.dev/health` - Health check
- `https://your-app-name.fly.dev/detect` - Room detection
- `https://your-app-name.fly.dev/detect/rust-floodfill` - Flood fill algorithm
- `https://your-app-name.fly.dev/detect/connected-components` - Connected components
- `https://your-app-name.fly.dev/validate/gpt4o` - GPT-4o validation

## Frontend Configuration

Update your frontend to point to the deployed backend:

```javascript
// In leptos-frontend/src/lib.rs
const BACKEND_URL = "https://your-app-name.fly.dev";

// Update all fetch calls:
Request::post(&format!("{}/detect/rust-floodfill", BACKEND_URL))
```

## Cost Optimization

- Free tier includes 3 shared-cpu-1x VMs with 256MB RAM
- Auto-scaling to zero reduces costs when idle
- Monitor usage: `fly dashboard`

## Notes

- The Python detection endpoint (`/detect/python-cc`) is NOT included in the Docker build
- Only pure Rust implementations are deployed
- OpenCV is not required for the deployed backend
- The frontend (Leptos) needs separate hosting (Vercel, Netlify, etc.) or can be served statically
