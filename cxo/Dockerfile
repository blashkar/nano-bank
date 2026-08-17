# Build from the REPO ROOT so the shared csuite package is in context:
#   docker build -f cxo/Dockerfile -t nano-cxo:dev .
# (cxo consumes the cx metrics MCP over HTTP, so the cx package is NOT needed here.)
FROM python:3.12-slim
WORKDIR /app
COPY cxo/requirements.txt /app/requirements.txt
RUN pip install --no-cache-dir -r requirements.txt
COPY csuite /app/csuite
COPY cxo /app/cxo
ENV PYTHONUNBUFFERED=1
CMD ["python", "-m", "cxo.api_main"]
