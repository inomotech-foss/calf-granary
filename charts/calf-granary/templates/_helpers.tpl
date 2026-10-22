{{- define "granary.name" -}}
{{- .Release.Name | trunc 50 | trimSuffix "-" -}}
{{- end -}}

{{- define "granary.labels" -}}
app.kubernetes.io/name: calf-granary
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end -}}

{{- define "granary.selector" -}}
app.kubernetes.io/name: calf-granary
app.kubernetes.io/instance: {{ .root.Release.Name }}
app.kubernetes.io/component: {{ .component }}
{{- end -}}

{{- define "granary.image" -}}
{{ .Values.image.repository }}:{{ .Values.image.tag | default .Chart.AppVersion }}
{{- end -}}

{{- define "granary.env" -}}
- name: GRANARY_CONFIG
  value: /etc/granary/config.toml
- name: GRANARY_DATABASE_URL
  valueFrom:
    secretKeyRef:
      name: {{ required "database.secretName is required" .Values.database.secretName }}
      key: {{ .Values.database.urlKey }}
{{- with .Values.database.caKey }}
- name: GRANARY_DATABASE_CA
  value: /etc/granary/database/{{ . }}
{{- end }}
- name: HOSTNAME
  valueFrom:
    fieldRef:
      fieldPath: metadata.name
{{- with .Values.otel.endpoint }}
- name: OTEL_EXPORTER_OTLP_ENDPOINT
  value: {{ . | quote }}
{{- end }}
{{- with .Values.otel.metricsEndpoint }}
- name: OTEL_EXPORTER_OTLP_METRICS_ENDPOINT
  value: {{ . | quote }}
{{- end }}
{{- with .Values.extraEnv }}
{{ toYaml . }}
{{- end }}
{{- end -}}

{{- define "granary.migrate" -}}
- name: migrate
  image: {{ include "granary.image" . }}
  imagePullPolicy: {{ .Values.image.pullPolicy }}
  args: ["migrate"]
  env:
    {{- include "granary.env" . | nindent 4 }}
  securityContext:
    {{- toYaml .Values.securityContext | nindent 4 }}
  volumeMounts:
    {{- include "granary.volumeMounts" . | nindent 4 }}
{{- end -}}

{{- define "granary.envFrom" -}}
- secretRef:
    name: {{ required "storage.secretName is required" .Values.storage.secretName }}
{{- end -}}

{{- define "granary.volumeMounts" -}}
- name: config
  mountPath: /etc/granary/config.toml
  subPath: config.toml
  readOnly: true
{{- if .Values.database.caKey }}
- name: database
  mountPath: /etc/granary/database
  readOnly: true
{{- end }}
{{- if .Values.signingKeys.secretName }}
- name: signing-keys
  mountPath: /etc/granary/keys
  readOnly: true
{{- end }}
{{- end -}}

{{- define "granary.volumes" -}}
- name: config
  configMap:
    name: {{ include "granary.name" . }}
{{- with .Values.database.caKey }}
- name: database
  secret:
    secretName: {{ $.Values.database.secretName }}
    items:
      - key: {{ . }}
        path: {{ . }}
{{- end }}
{{- with .Values.signingKeys.secretName }}
- name: signing-keys
  secret:
    secretName: {{ . }}
{{- end }}
{{- end -}}
