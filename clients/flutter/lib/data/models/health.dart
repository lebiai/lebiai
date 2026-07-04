import 'package:flutter/foundation.dart';

/// Response of `GET /api/v1/health`.
@immutable
class HealthResponse {
  const HealthResponse({required this.status});

  final String status;

  factory HealthResponse.fromJson(Map<String, dynamic> json) {
    return HealthResponse(status: json['status'] as String);
  }
}
