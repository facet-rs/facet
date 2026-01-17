+++
title = "Terraform"
weight = 7
slug = "terraform"
insert_anchor_links = "heading"
+++

Terraform infrastructure in HCL vs Styx.

```compare
/// hcl
terraform {
  required_version = ">= 1.0"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
  }

  backend "s3" {
    bucket = "my-terraform-state"
    key    = "prod/terraform.tfstate"
    region = "us-east-1"
  }
}

provider "aws" {
  region = var.aws_region

  default_tags {
    tags = {
      Environment = var.environment
      ManagedBy   = "terraform"
    }
  }
}

variable "aws_region" {
  type        = string
  default     = "us-east-1"
  description = "AWS region for resources"
}

variable "environment" {
  type = string
}

resource "aws_vpc" "main" {
  cidr_block           = "10.0.0.0/16"
  enable_dns_hostnames = true
  enable_dns_support   = true

  tags = {
    Name = "${var.environment}-vpc"
  }
}

resource "aws_subnet" "public" {
  count             = 3
  vpc_id            = aws_vpc.main.id
  cidr_block        = cidrsubnet(aws_vpc.main.cidr_block, 8, count.index)
  availability_zone = data.aws_availability_zones.available.names[count.index]

  map_public_ip_on_launch = true

  tags = {
    Name = "${var.environment}-public-${count.index + 1}"
    Type = "public"
  }
}

output "vpc_id" {
  value       = aws_vpc.main.id
  description = "The ID of the VPC"
}
/// styx
terraform {
  required_version ">= 1.0"

  required_providers aws>{
    source hashicorp/aws
    version "~> 5.0"
  }

  backend.s3 {
    bucket my-terraform-state
    key prod/terraform.tfstate
    region us-east-1
  }
}

provider.aws {
  region "${var.aws_region}"

  default_tags tags>{
    Environment "${var.environment}"
    ManagedBy terraform
  }
}

variable.aws_region {
  type string
  default us-east-1
  description "AWS region for resources"
}

variable.environment type>string

resource.aws_vpc.main {
  cidr_block 10.0.0.0/16
  enable_dns_hostnames true
  enable_dns_support true
  tags Name>"${var.environment}-vpc"
}

resource.aws_subnet.public {
  count 3
  vpc_id "${aws_vpc.main.id}"
  cidr_block "${cidrsubnet(aws_vpc.main.cidr_block, 8, count.index)}"
  availability_zone "${data.aws_availability_zones.available.names[count.index]}"
  map_public_ip_on_launch true
  tags {
    Name "${var.environment}-public-${count.index + 1}"
    Type public
  }
}

output.vpc_id {
  value "${aws_vpc.main.id}"
  description "The ID of the VPC"
}
```
