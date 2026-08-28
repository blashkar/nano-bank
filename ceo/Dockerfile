# Build from the REPO ROOT so the shared csuite package is in context:
#   docker build -f ceo/Dockerfile -t nano-ceo:dev .
# (ceo consults its peers over HTTP and writes only the agent_action_ledger via
# psycopg2; no domain MCP package is vendored here.)
FROM python:3.12-slim
WORKDIR /app
COPY ceo/requirements.txt /app/requirements.txt
RUN pip install --no-cache-dir -r requirements.txt
COPY csuite /app/csuite
COPY ceo /app/ceo
ENV PYTHONUNBUFFERED=1
CMD ["python", "-m", "ceo.api_main"]
