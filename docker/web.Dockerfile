FROM node:24-alpine
WORKDIR /workspace/frontend/web
COPY frontend/web/package*.json ./
RUN npm install
COPY frontend/web ./
EXPOSE 5173
CMD ["npm", "run", "dev", "--", "--host", "0.0.0.0"]

