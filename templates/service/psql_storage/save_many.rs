        let mut results = Vec::with_capacity(data.len());
        for item in data {
            results.push(self.save_one(item).await?);
        }
        Ok(results)