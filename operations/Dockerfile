FROM python:3.12-slim
WORKDIR /app
COPY requirements.txt /app/requirements.txt
RUN pip install --no-cache-dir -r requirements.txt
COPY . /app/operations
ENV PYTHONUNBUFFERED=1
CMD ["python", "-m", "operations.mcp_server"]
