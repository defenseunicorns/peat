"""HIVE port-ops simulation agent framework.

Provides LLM-tiered agent decision making for simulated port operations.
Sensor agents (load cells, RFID readers, etc.) use pure rule-based logic
while equipment and human agents can optionally use LLM reasoning.

See ADR-022 and Addendum A (Option C: Tiered) for architecture context.
"""
